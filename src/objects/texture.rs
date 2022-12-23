use std::fs::File;
use std::ptr::copy_nonoverlapping as memcpy;

use vulkanalia::prelude::v1_0::*;
use anyhow::{Result, Ok};

use crate::AppData;
use crate::shared_memory::*;

#[derive(Copy, Clone, Debug, Default)]
pub struct Texture {
    mip_levels: u32,

    pub image: vk::Image,
    pub image_memory: vk::DeviceMemory,
    pub image_view: vk::ImageView,
}

impl Texture {
    pub unsafe fn from_filepath(
        filepath: String,
        instance: &Instance,
        device: &Device,
        data: &AppData
    ) -> Result<Self> {
        let image = File::open(filepath)?;
        let decoder = png::Decoder::new(image);
        let mut reader = decoder.read_info()?;

        let mut pixels = vec![0; reader.info().raw_bytes()];
        reader.next_frame(&mut pixels)?;

        let size = reader.info().raw_bytes() as u64;
        let (width, height) = reader.info().size();
        let mip_levels = (width.max(height) as f32).log2().floor() as u32 + 1;

        let (staging_buffer, staging_buffer_memory) = create_buffer(
            instance, device, data,
            size,
            vk::BufferUsageFlags::TRANSFER_SRC,
            vk::MemoryPropertyFlags::HOST_COHERENT | vk::MemoryPropertyFlags::HOST_VISIBLE
        )?;
        
        let memory = device.map_memory(
            staging_buffer_memory, 0, size, vk::MemoryMapFlags::empty()
        )?;
        
        memcpy(pixels.as_ptr(), memory.cast(), pixels.len());
        device.unmap_memory(staging_buffer_memory);
        
        // TODO: add format determination
        // println!("{} bit - {:?}", reader.info().bit_depth as usize, reader.info().color_type);

        let (image, image_memory) = create_image(
            instance, device, data,
            width, height,
            mip_levels,
            vk::SampleCountFlags::_1,
            vk::Format::R8G8B8A8_SRGB,
            vk::ImageTiling::OPTIMAL,
            vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::TRANSFER_SRC,
            vk::MemoryPropertyFlags::DEVICE_LOCAL
        )?;

        transition_image_layout(
            device, data,
            image,
            vk::Format::R8G8B8A8_SRGB,
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            mip_levels
        )?;

        copy_buffer_to_image(device, data, staging_buffer, image, width, height)?;

        device.destroy_buffer(staging_buffer, None);
        device.free_memory(staging_buffer_memory, None);

        generate_mipmaps(
            instance, device, data,
            image,
            vk::Format::R8G8B8A8_SRGB,
            width, height,
            mip_levels
        )?;

        let image_view = create_image_view(device, image, vk::Format::R8G8B8A8_SRGB, vk::ImageAspectFlags::COLOR, mip_levels)?;

        Ok(Self {
            mip_levels,
            image,
            image_memory,
            image_view
        })
    }

    pub unsafe fn destroy(&self, device: &Device) {
        device.destroy_image_view(self.image_view, None);
        device.destroy_image(self.image, None);
        device.free_memory(self.image_memory, None);
    }
}
