const MAX_LOD: usize = 10;

fn lod_ranges<const N: usize>(min_lod_distance: f32) -> [f32; N] {
    let mut lod_ranges = [0.0; N];
    for (i, item) in lod_ranges.iter_mut().enumerate() {
        *item = min_lod_distance * usize::pow(2, i as u32) as f32;
    }
    lod_ranges
}

struct Node {
    min: f32,
    max: f32,
}

fn children(node: usize) -> [usize; 4] {
    let base = node * 4;
    [base + 1, base + 2, base + 3, base + 4]
}

struct Frustum;

struct Terrain {
    ranges: [f32; MAX_LOD],
    tree: Vec<Node>,
    draw_list: Vec<usize>,

    min_lod_distance: f32, // = 15.0;
    frustum: Frustum,
}

impl Terrain {
    // Call function using top node, going from lod_level tree_depth - 1 down to zero.
    fn select_lods(&mut self, node: usize, lod: usize) -> bool {
        // If tree_depth is greater than lod_levels traverse down tree.
        if lod > MAX_LOD {
            for child in children(node) {
                self.select_lods(child, lod - 1);
            }
            return true;
        }

        if !self.sphere_intersect(node, self.ranges[lod]) {
            // Skip nodes node not intersecting current lod_range.
            return false;
        }

        if !self.frustum_intersect(node, &self.frustum) {
            // Skip nodes node not visible to camera.
            return true;
        }

        if lod == 0 {
            // Always add LOD0 within range.
            self.draw_list.push(node);
            true
        } else {
            if !self.sphere_intersect(node, self.ranges[lod - 1]) {
                // We now know this node is only covering one lodrange.
                self.draw_list.push(node);
            } else {
                // If node is within LOD and also within range of LOD - 1
                // we add children of node that only covers LOD and skip
                // children that covers LOD - 1
                for child in children(node) {
                    if !self.select_lods(child, lod - 1) {
                        // Add child to draw list that doesn't cover LOD - 1
                        self.draw_list.push(child);
                    }
                }
            }
            true
        }
    }

    fn sphere_intersect(&self, node: usize, radius: f32) -> bool {
        todo!()
    }

    fn frustum_intersect(&self, node: usize, frustum: &Frustum) -> bool {
        todo!()
    }
}

fn clamp(v: f32, min: f32, max: f32) -> f32 {
    f32::clamp(v, min, max)
}

fn fract(v: Vec2) -> Vec2 {
    v.fract()
}

fn select(if_false: f32, if_true: f32, cond: bool) -> f32 {
    if cond {
        if_true
    } else {
        if_false
    }
}

use glam::Vec2;

// Calculates the morph value from 0.0 to 1.0 given the distance
// from the camera to the vertex, and the current LOD level.
fn morph_value(ranges: &[f32], distance: f32, lod: usize) -> f32 {
    let low = select(0.0, ranges[lod - 1], lod != 0);
    let high = ranges[lod];
    let delta = high - low;
    let factor = (distance - low) / delta;
    return clamp(factor / 0.5 - 1.0, 0.0, 1.0);
}

// Morphs the vertex position in object-space given its
// position in the mesh grid ranging from 0.0 to the mesh grid dimensions.
// All positions only contain its x and z values, y values will be
// retrieved later from the height texture.
fn morph_vertex(vertex: Vec2, mesh_pos: Vec2, mesh_dim: f32, morph_value: f32) -> Vec2 {
    return vertex - fract(mesh_pos * mesh_dim * 0.5) * 2.0 / mesh_dim * morph_value;
}
