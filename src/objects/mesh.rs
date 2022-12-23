use std::io::BufReader;
use std::fs::File;
use std::collections::HashMap;
use std::mem::size_of;
use std::ptr::copy_nonoverlapping as memcpy;

use anyhow::Result;
use vulkanalia::prelude::v1_0::*;
use nalgebra_glm as glm;

use crate::AppData;
use crate::shared_memory::*;

use super::vertex::Vertex;

#[derive(Clone, Debug, Default)]
pub struct Mesh {
    vertices: Vec<Vertex>,
    pub indices: Vec<u32>,

    pub vertex_buffer: vk::Buffer,
    pub vertex_buffer_memory: vk::DeviceMemory,

    pub index_buffer: vk::Buffer,
    pub index_buffer_memory: vk::DeviceMemory
}

impl Mesh {
    pub unsafe fn from_filepath(
        filepath: String,
        instance: &Instance,
        device: &Device,
        data: &AppData
    ) -> Result<Self> {
        let mut reader = BufReader::new(File::open(filepath)?);

        let (models, _) = tobj::load_obj_buf(
            &mut reader, 
            &tobj::LoadOptions {
                triangulate: true,
                single_index: true,
                ..Default::default()
            }, 

            |_| Ok(Default::default())
        )?;

        let mut unique_vertices = HashMap::new();

        let mut vertices = vec![];
        let mut indices = vec![];

        for model in &models {
            for index in &model.mesh.indices {
                let pos_offset = (3 * index) as usize;
                let tex_coords_offset = (2 * index) as usize;

                let mut vtx_color = glm::vec3(1.0, 1.0, 1.0);
                if model.mesh.vertex_color.len() > 0 {
                    let vtx_color_offset = (3 * index) as usize;
                    vtx_color = glm::vec3(
                        model.mesh.vertex_color[vtx_color_offset],
                        model.mesh.vertex_color[vtx_color_offset + 1],
                        model.mesh.vertex_color[vtx_color_offset + 2],
                    );
                }

                let mut normal = glm::vec3(0.0, 0.0, 1.0);
                if model.mesh.normals.len() > 0 {
                    let normal_offset = (3 * index) as usize;
                    normal = glm::vec3(
                        model.mesh.normals[normal_offset],
                        model.mesh.normals[normal_offset + 1],
                        model.mesh.normals[normal_offset + 2]
                    );
                }

                let vertex = Vertex {
                    pos: glm::vec3(
                        model.mesh.positions[pos_offset],
                        model.mesh.positions[pos_offset + 1],
                        model.mesh.positions[pos_offset + 2],
                    ),
                    color: vtx_color,
                    tex_coord: glm::vec2(
                        model.mesh.texcoords[tex_coords_offset],
                        1.0 - model.mesh.texcoords[tex_coords_offset + 1]
                    ),
                    normal
                };

                if let Some(index) = unique_vertices.get(&vertex) {
                    indices.push(*index as u32);
                } else {
                    let index = vertices.len();
                    unique_vertices.insert(vertex, index);
                    vertices.push(vertex);
                    indices.push(index as u32)
                }
            }
        }

        let (vertex_buffer, vertex_buffer_memory) = Mesh::create_vertex_buffer(instance, device, data, &vertices)?;
        let (index_buffer, index_buffer_memory) = Mesh::create_index_buffer(instance, device, data, &indices)?;

        Ok(Mesh{
            vertices, indices,
            vertex_buffer,
            vertex_buffer_memory,
            index_buffer,
            index_buffer_memory
        })
    }

    // TODO: merge functions?
    unsafe fn create_vertex_buffer(
        instance: &Instance,
        device: &Device,
        data: &AppData,
        vertices: &Vec<Vertex>
    ) -> Result<(vk::Buffer, vk::DeviceMemory)> {
        let size = (size_of::<Vertex>() * vertices.len()) as u64;
    
        let (staging_buffer, staging_buffer_memory) = create_buffer(
            instance, device, data, size,
            vk::BufferUsageFlags::TRANSFER_SRC,
            vk::MemoryPropertyFlags::HOST_COHERENT | vk::MemoryPropertyFlags::HOST_VISIBLE
        )?;
    
        let memory = device.map_memory(staging_buffer_memory, 0, size, vk::MemoryMapFlags::empty())?;
        memcpy(vertices.as_ptr(), memory.cast(), vertices.len());
    
        let (vertex_buffer, vertex_buffer_memory) = create_buffer(instance, device, data, size,
            vk::BufferUsageFlags::TRANSFER_DST | vk::BufferUsageFlags::VERTEX_BUFFER,
            vk::MemoryPropertyFlags::DEVICE_LOCAL
        )?;
    
        let buffer = vertex_buffer;
        let buffer_memory = vertex_buffer_memory;
    
        copy_buffer(device, data, staging_buffer, buffer, size)?;
    
        device.destroy_buffer(staging_buffer, None);
        device.free_memory(staging_buffer_memory, None);
    
        Ok((buffer, buffer_memory))
    }

    unsafe fn create_index_buffer(
        instance: &Instance,
        device: &Device,
        data: &AppData,
        indices: &Vec<u32>
    ) -> Result<(vk::Buffer, vk::DeviceMemory)> {
        let size = (size_of::<u32>() * indices.len()) as u64;
    
        let (staging_buffer, staging_buffer_memory) = create_buffer(
            instance, device, data, size,
            vk::BufferUsageFlags::TRANSFER_SRC,
            vk::MemoryPropertyFlags::HOST_COHERENT | vk::MemoryPropertyFlags::HOST_VISIBLE
        )?;
    
        let memory = device.map_memory(staging_buffer_memory, 0,  size, vk::MemoryMapFlags::empty())?;
        memcpy(indices.as_ptr(), memory.cast(), indices.len());
    
        let (index_buffer, index_buffer_memory) = create_buffer(instance, device, data, size,
            vk::BufferUsageFlags::TRANSFER_DST | vk::BufferUsageFlags::INDEX_BUFFER,
            vk::MemoryPropertyFlags::DEVICE_LOCAL
        )?;
    
        let index_buffer = index_buffer;
        let index_buffer_memory = index_buffer_memory;
    
        copy_buffer(device, data, staging_buffer, index_buffer, size)?;
    
        device.destroy_buffer(staging_buffer, None);
        device.free_memory(staging_buffer_memory, None);
    
        Ok((index_buffer, index_buffer_memory))
    }

    pub unsafe fn destroy(&mut self, device: &Device) {
        device.destroy_buffer(self.vertex_buffer, None);
        device.free_memory(self.vertex_buffer_memory, None);
        device.destroy_buffer(self.index_buffer, None);
        device.free_memory(self.index_buffer_memory, None);
    }
    
}