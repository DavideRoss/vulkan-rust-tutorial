use std::fs::File;
use std::ptr::copy_nonoverlapping as memcpy;

use vulkanalia::prelude::v1_0::*;
use anyhow::{Result, Ok};

use crate::AppData;
use crate::shared_memory::*;

#[derive(Clone, Debug)]
pub struct Texture {
    image: vk::Image,
    image_layout: vk::ImageLayout,
    device_memory: vk::DeviceMemory,
    pub image_view: vk::ImageView,

    width: u32,
    height: u32,
    mip_levels: u32,
    layer_count: u32,

    descriptor: vk::DescriptorImageInfo,
    sampler: Option<vk::Sampler>
}

impl Texture {
    fn update_descriptor(&mut self) {
        self.descriptor.sampler = self.sampler.unwrap_or(vk::Sampler::null());
        self.descriptor.image_view = self.image_view;
        self.descriptor.image_layout = self.image_layout;
    }

    pub unsafe fn destroy(&self, device: &Device) {
        device.destroy_image_view(self.image_view, None);
        device.destroy_image(self.image, None);

        if self.sampler.is_some() {
            device.destroy_sampler(self.sampler.unwrap(), None);
        }

        device.free_memory(self.device_memory, None);
    }
}

#[derive(Clone, Debug)]
pub struct Texture2D {
    pub texture: Texture
}

impl Texture2D {
    pub unsafe fn load_from_file(
        instance: &Instance,
        device: &Device,
        data: &AppData, // TODO: move appdata to device?
        filename: &str,
        format: vk::Format,
        // copy_queue: vk::Queue, TODO: restore queue while removing data
        image_usage_flags: Option<vk::ImageUsageFlags>, // VK_IMAGE_USAGE_SAMPLED_BIT
        image_layout: Option<vk::ImageLayout> // VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL // TODO: explicit optional params
    ) -> Result<Self> {
        let image = File::open(filename).unwrap();
        let decoder = png::Decoder::new(image);
        let mut reader = decoder.read_info().unwrap();

        let mut pixels = vec![0; reader.info().raw_bytes()];
        reader.next_frame(&mut pixels)?;

        let size = reader.info().raw_bytes() as u64;
        let (width, height) = reader.info().size();
        let mip_levels = (width.max(height) as f32).log2().floor() as u32 + 1;

        // TODO: move to dedicated create_command_buffer function
        let cmd_info = vk::CommandBufferAllocateInfo::builder()
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_pool(data.command_pool)
            .command_buffer_count(1)
            .build();

        let copy_cmd = device.allocate_command_buffers(&cmd_info)?[0];

        // Begin command buffer
        let begin_info = vk::CommandBufferBeginInfo::builder().build();
        device.begin_command_buffer(copy_cmd, &begin_info)?;

        let buffer_info = vk::BufferCreateInfo::builder()
            .size(size)
            .usage(vk::BufferUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .build();

        let staging_buffer = device.create_buffer(&buffer_info, None)?;
        let mut mem_reqs = device.get_buffer_memory_requirements(staging_buffer);

        let mut mem_info = vk::MemoryAllocateInfo::builder()
            .allocation_size(size)
            .memory_type_index(get_memory_type_index(
                instance, data,
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
                mem_reqs
            )?)
            .build();

        let staging_mem = device.allocate_memory(&mem_info, None)?;
        device.bind_buffer_memory(staging_buffer, staging_mem, 0)?;

        let mem_dst = device.map_memory(staging_mem, 0, mem_reqs.size, vk::MemoryMapFlags::empty())?;
        memcpy(pixels.as_ptr(), mem_dst.cast(), pixels.len());
        device.unmap_memory(staging_mem);

        let mut buffer_copy_regions: Vec<vk::BufferImageCopy> = vec![];
        let mut offset: u64 = 0;
        
        for i in 0..mip_levels {
            let subres = vk::ImageSubresourceLayers::builder()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .mip_level(i)
                .base_array_layer(0)
                .layer_count(1)
                .build();

            let buffer_copy_region = vk::BufferImageCopy::builder()
                .image_subresource(subres)
                .image_extent(vk::Extent3D { width, height, depth: 1 })
                .buffer_offset(offset)
                .build();

            buffer_copy_regions.push(buffer_copy_region);
            offset += size;
        }

        let image_info = vk::ImageCreateInfo::builder()
            .image_type(vk::ImageType::_2D)
            .format(format)
            .mip_levels(mip_levels)
            .array_layers(1)
            .samples(vk::SampleCountFlags::_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .extent(vk::Extent3D { width, height, depth: 1 })
            .usage(image_usage_flags.unwrap_or(vk::ImageUsageFlags::SAMPLED) | vk::ImageUsageFlags::TRANSFER_DST)
            .build();

        let image = device.create_image(&image_info, None)?;

        mem_reqs = device.get_image_memory_requirements(image);
        mem_info = vk::MemoryAllocateInfo::builder()
            .allocation_size(mem_reqs.size)
            .memory_type_index(get_memory_type_index(
                instance, data,
                vk::MemoryPropertyFlags::DEVICE_LOCAL,
                mem_reqs
            )?)
            .build();

        let device_memory = device.allocate_memory(&mem_info, None)?;
        device.bind_image_memory(image, device_memory, 0)?;

        let subres_range = vk::ImageSubresourceRange::builder()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .base_mip_level(0)
            .level_count(mip_levels)
            .layer_count(1)
            .build();
        
        // Barrier
        {
            let image_memory_barrier = vk::ImageMemoryBarrier::builder()
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .src_access_mask(vk::AccessFlags::empty())
                .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .image(image)
                .subresource_range(subres_range)
                .build();

            device.cmd_pipeline_barrier(
                copy_cmd,
                vk::PipelineStageFlags::ALL_COMMANDS,
                vk::PipelineStageFlags::ALL_COMMANDS,
                vk::DependencyFlags::empty(),
                &[] as &[vk::MemoryBarrier],
                &[] as &[vk::BufferMemoryBarrier],
                &[image_memory_barrier]
            );
        }

        // End command buffer and flush
        // TODO: move to separate function
        device.end_command_buffer(copy_cmd)?;

        let submit_info = vk::SubmitInfo::builder().command_buffers(&[copy_cmd]).build();
        let fence_info = vk::FenceCreateInfo::builder().build();
        let fence = device.create_fence(&fence_info, None)?;

        device.queue_submit(data.graphics_queue, &[submit_info], fence)?;
        device.wait_for_fences(&[fence], true, 100000000000)?;
        device.destroy_fence(fence, None);
        device.free_command_buffers(data.command_pool, &[copy_cmd]);

        // Clean up staging
        device.free_memory(staging_mem, None);
        device.destroy_buffer(staging_buffer, None);

        let sampler_info = vk::SamplerCreateInfo::builder()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
            .address_mode_u(vk::SamplerAddressMode::REPEAT)
            .address_mode_v(vk::SamplerAddressMode::REPEAT)
            .address_mode_w(vk::SamplerAddressMode::REPEAT)
            .mip_lod_bias(0.0)
            .compare_op(vk::CompareOp::NEVER)
            .min_lod(0.0)
            .max_lod(mip_levels as f32)
            .max_anisotropy(16.0) // TODO: check if device supports aniso
            .anisotropy_enable(true)
            .border_color(vk::BorderColor::FLOAT_OPAQUE_WHITE)
            .build();

        let sampler = device.create_sampler(&sampler_info, None)?;

        let image_view_subres_range = vk::ImageSubresourceRange::builder()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .base_mip_level(0)
            .level_count(mip_levels)
            .base_array_layer(0)
            .layer_count(1)
            .build();

        let image_view_info = vk::ImageViewCreateInfo::builder()
            .view_type(vk::ImageViewType::_2D)
            .format(format)
            .components(vk::ComponentMapping {
                r: vk::ComponentSwizzle::R,
                g: vk::ComponentSwizzle::G,
                b: vk::ComponentSwizzle::B,
                a: vk::ComponentSwizzle::A
            })
            .subresource_range(image_view_subres_range)
            .image(image)
            .build();

        let image_view = device.create_image_view(&image_view_info, None)?;

        let descriptor = vk::DescriptorImageInfo::builder()
            .sampler(sampler)
            .image_view(image_view)
            .image_layout(image_layout.unwrap_or(vk::ImageLayout::READ_ONLY_OPTIMAL))
            .build();

        let texture = Texture {
            image,
            image_layout: image_layout.unwrap_or(vk::ImageLayout::READ_ONLY_OPTIMAL),
            image_view,
            device_memory,

            width, height, mip_levels,
            layer_count: 1,

            sampler: Some(sampler),
            descriptor
        };

        Ok(Texture2D {
            texture
        })
    }
}

/*

- load texture in library
- get width, height, mip levels
- get VkFormatProperties
- create copy command buffer
- create staging buffer
- get memory requirements for staging buffer
- allocate and bind staging buffer
- memcpy inside this buffer
- unmap staging memory
- create mip levels
    - for each mip level, create a new VkBufferImageCopy
- create image
- get memory requirement for image
- allocate and bind device memory for images

 */

// #[derive(Copy, Clone, Debug, Default)]
// pub struct Texture {
//     mip_levels: u32,

//     pub image: vk::Image,
//     pub image_memory: vk::DeviceMemory,
//     pub image_view: vk::ImageView,
// }

// impl Texture {
//     pub unsafe fn from_filepath(
//         filepath: String,
//         instance: &Instance,
//         device: &Device,
//         data: &AppData
//     ) -> Result<Self> {
//         let image = File::open(filepath)?;
//         let decoder = png::Decoder::new(image);
//         let mut reader = decoder.read_info()?;

//         let mut pixels = vec![0; reader.info().raw_bytes()];
//         reader.next_frame(&mut pixels)?;

//         let size = reader.info().raw_bytes() as u64;
//         let (width, height) = reader.info().size();
//         let mip_levels = (width.max(height) as f32).log2().floor() as u32 + 1;

//         let (staging_buffer, staging_buffer_memory) = create_buffer(
//             instance, device, data,
//             size,
//             vk::BufferUsageFlags::TRANSFER_SRC,
//             vk::MemoryPropertyFlags::HOST_COHERENT | vk::MemoryPropertyFlags::HOST_VISIBLE
//         )?;
        
//         let memory = device.map_memory(
//             staging_buffer_memory, 0, size, vk::MemoryMapFlags::empty()
//         )?;
        
//         memcpy(pixels.as_ptr(), memory.cast(), pixels.len());
//         device.unmap_memory(staging_buffer_memory);
        
//         // TODO: add format determination
//         // println!("{} bit - {:?}", reader.info().bit_depth as usize, reader.info().color_type);

//         let (image, image_memory) = create_image(
//             instance, device, data,
//             width, height,
//             mip_levels,
//             vk::SampleCountFlags::_1,
//             vk::Format::R8G8B8A8_SRGB,
//             vk::ImageTiling::OPTIMAL,
//             vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::TRANSFER_SRC,
//             vk::MemoryPropertyFlags::DEVICE_LOCAL
//         )?;

//         transition_image_layout(
//             device, data,
//             image,
//             vk::Format::R8G8B8A8_SRGB,
//             vk::ImageLayout::UNDEFINED,
//             vk::ImageLayout::TRANSFER_DST_OPTIMAL,
//             mip_levels
//         )?;

//         copy_buffer_to_image(device, data, staging_buffer, image, width, height)?;

//         device.destroy_buffer(staging_buffer, None);
//         device.free_memory(staging_buffer_memory, None);

//         generate_mipmaps(
//             instance, device, data,
//             image,
//             vk::Format::R8G8B8A8_SRGB,
//             width, height,
//             mip_levels
//         )?;

//         let image_view = create_image_view(device, image, vk::Format::R8G8B8A8_SRGB, vk::ImageAspectFlags::COLOR, mip_levels)?;

//         Ok(Self {
//             mip_levels,
//             image,
//             image_memory,
//             image_view
//         })
//     }

//     pub unsafe fn destroy(&self, device: &Device) {
//         device.destroy_image_view(self.image_view, None);
//         device.destroy_image(self.image, None);
//         device.free_memory(self.image_memory, None);
//     }
// }
