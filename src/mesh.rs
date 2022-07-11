use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Vertex {
    pub pos: [i8; 4],
    pub normal: [i8; 4],
}

pub fn vertex(pos: [i8; 3], nor: [i8; 3]) -> Vertex {
    Vertex {
        pos: [pos[0], pos[1], pos[2], 1],
        normal: [nor[0], nor[1], nor[2], 0],
    }
}

pub fn create_cube() -> (Vec<Vertex>, Vec<u16>) {
    let vertex_data = [
        // top (0, 0, 1)
        vertex([-1, -1, 1], [0, 0, 1]),
        vertex([1, -1, 1], [0, 0, 1]),
        vertex([1, 1, 1], [0, 0, 1]),
        vertex([-1, 1, 1], [0, 0, 1]),
        // bottom (0, 0, -1)
        vertex([-1, 1, -1], [0, 0, -1]),
        vertex([1, 1, -1], [0, 0, -1]),
        vertex([1, -1, -1], [0, 0, -1]),
        vertex([-1, -1, -1], [0, 0, -1]),
        // right (1, 0, 0)
        vertex([1, -1, -1], [1, 0, 0]),
        vertex([1, 1, -1], [1, 0, 0]),
        vertex([1, 1, 1], [1, 0, 0]),
        vertex([1, -1, 1], [1, 0, 0]),
        // left (-1, 0, 0)
        vertex([-1, -1, 1], [-1, 0, 0]),
        vertex([-1, 1, 1], [-1, 0, 0]),
        vertex([-1, 1, -1], [-1, 0, 0]),
        vertex([-1, -1, -1], [-1, 0, 0]),
        // front (0, 1, 0)
        vertex([1, 1, -1], [0, 1, 0]),
        vertex([-1, 1, -1], [0, 1, 0]),
        vertex([-1, 1, 1], [0, 1, 0]),
        vertex([1, 1, 1], [0, 1, 0]),
        // back (0, -1, 0)
        vertex([1, -1, 1], [0, -1, 0]),
        vertex([-1, -1, 1], [0, -1, 0]),
        vertex([-1, -1, -1], [0, -1, 0]),
        vertex([1, -1, -1], [0, -1, 0]),
    ];

    let index_data: &[u16] = &[
        0, 1, 2, 2, 3, 0, // top
        4, 5, 6, 6, 7, 4, // bottom
        8, 9, 10, 10, 11, 8, // right
        12, 13, 14, 14, 15, 12, // left
        16, 17, 18, 18, 19, 16, // front
        20, 21, 22, 22, 23, 20, // back
    ];

    (vertex_data.to_vec(), index_data.to_vec())
}

pub fn _create_plane(size: i8) -> (Vec<Vertex>, Vec<u16>) {
    let vertex_data = [
        vertex([size, -size, 0], [0, 1, 0]),
        vertex([size, size, 0], [0, 1, 0]),
        vertex([-size, -size, 0], [0, 1, 0]),
        vertex([-size, size, 0], [0, 1, 0]),
    ];

    let index_data: &[u16] = &[0, 1, 2, 2, 1, 3];

    (vertex_data.to_vec(), index_data.to_vec())
}

pub fn create_terrain(x_size: usize, z_size: usize) -> (Vec<Vertex>, Vec<u16>) {
    let (x_size, z_size) = (x_size * 2, z_size * 2);

    let mut vertices = Vec::with_capacity((x_size + 1) * (z_size + 1));
    let mut indices = Vec::with_capacity(x_size * z_size * 6);

    let ox = x_size as i8 / 2;
    let oz = z_size as i8 / 2;
    for z in 0..=z_size as i8 {
        for x in 0..=x_size as i8 {
            vertices.push(vertex([x - ox, 0, z - oz], [0, 1, 0]));
        }
    }

    for z in 0..z_size {
        for x in 0..x_size {
            let (z0, x0) = (z * (x_size + 1), x);
            let (z1, x1) = (z0 + x_size + 1, x0 + 1);

            indices.extend_from_slice(&[
                (z1 + x0) as _,
                (z1 + x1) as _,
                (z0 + x0) as _,
                //
                (z1 + x1) as _,
                (z0 + x1) as _,
                (z0 + x0) as _,
            ]);
        }
    }

    (vertices, indices)
}
