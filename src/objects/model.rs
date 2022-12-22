use std::io::BufReader;
use std::fs::File;
use std::collections::HashMap;
use std::collections::HashSet;

use anyhow::{anyhow, Result};

use nalgebra_glm as glm;

use super::vertex::Vertex;

#[derive(Copy, Clone, Debug, Default)]
pub struct Model {
    vertices: Vec<Vertex>,
    indices: Vec<u32>
}

impl Model {
    pub unsafe fn from_filepath(
        filepath: String
    ) -> Result<Self> {
        let mut reader = BufReader::new(File::open(filepath)?);

        let (models, _) = tobj::load_obj_buf(
            &mut reader, 
            &tobj::LoadOptions { triangulate: true, ..Default::default() }, 
            |_| Ok(Default::default())
        )?;

        let mut unique_vertices = HashMap::new();

        let vertices = vec![];
        let indices = vec![];

        for model in &models {
            for index in &model.mesh.indices {
                let pos_offset = (3 * index) as usize;
                let tex_coords_offset = (2 * index) as usize;

                let vertex = Vertex {
                    pos: glm::vec3(
                        model.mesh.positions[pos_offset],
                        model.mesh.positions[pos_offset + 1],
                        model.mesh.positions[pos_offset + 2],
                    ),
                    color: glm::vec3(1.0, 1.0, 1.0),
                    tex_coord: glm::vec2(
                        model.mesh.texcoords[tex_coords_offset],
                        1.0 - model.mesh.texcoords[tex_coords_offset + 1]
                    )
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

            Ok(Model{
                vertices, indices
            })
        }
    }
}