use geometry::Rect;

use crate::{Color, RoundedRectVertex, Vertex};

#[allow(clippy::module_name_repetitions)]
#[repr(u16)]
pub enum RenderGraphCommand {
    Root,
    DrawRect { first_index: u16, num_indices: u16 },
    DrawImmediate { first_index: u16, num_indices: u16 },
}

struct RenderGraphNode {
    next: u16,
    first_child: u16,
    last_child: u16,
    command: RenderGraphCommand,
}

#[allow(clippy::module_name_repetitions)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderGraphNodeId {
    index: u16,
}

impl RenderGraphNodeId {
    #[must_use]
    pub const fn root() -> Self {
        Self { index: 0 }
    }
}

pub struct RenderGraph {
    pub(crate) imm_indices: Vec<u16>,
    pub(crate) imm_vertices: Vec<Vertex>,
    pub(crate) imm_rect_vertices: Vec<RoundedRectVertex>,
    nodes: Vec<RenderGraphNode>,
}

impl Default for RenderGraph {
    fn default() -> Self {
        Self {
            imm_indices: Vec::new(),
            imm_vertices: Vec::new(),
            imm_rect_vertices: Vec::new(),
            nodes: vec![RenderGraphNode {
                next: 0,
                first_child: 0,
                last_child: 0,
                command: RenderGraphCommand::Root,
            }],
        }
    }
}

impl RenderGraph {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn get(&self, node: RenderGraphNodeId) -> &RenderGraphCommand {
        &self.nodes[node.index as usize].command
    }

    pub fn iter_children(
        &self,
        node: RenderGraphNodeId,
    ) -> impl Iterator<Item = RenderGraphNodeId> + '_ {
        struct It<'a> {
            current: u16,
            graph: &'a RenderGraph,
        }

        impl<'a> Iterator for It<'a> {
            type Item = RenderGraphNodeId;

            fn next(&mut self) -> Option<Self::Item> {
                if self.current == 0 {
                    None
                } else {
                    let node = RenderGraphNodeId {
                        index: self.current,
                    };
                    self.current = self.graph.nodes[node.index as usize].next;
                    Some(node)
                }
            }
        }

        It {
            current: self.nodes[node.index as usize].first_child,
            graph: self,
        }
    }

    /// Embeds the given mesh into the render graph for drawing. Use this for
    /// small meshes that change frequently (every frame or thereabouts), such
    /// as UI elements.
    ///
    /// ## Panics
    ///
    /// May panic if the number of vertices exceeds `u16::MAX`.
    pub fn draw_immediate(
        &mut self,
        parent: RenderGraphNodeId,
        vertices: &[Vertex],
        indices: &[u16],
    ) {
        let vertex_offset = self.imm_vertices.len();
        self.imm_vertices.extend_from_slice(vertices);

        let first_index = self.imm_indices.len();
        self.imm_indices.extend_from_slice(indices);
        for index in &mut self.imm_indices[first_index..] {
            *index = (*index as usize + vertex_offset).try_into().unwrap();
        }

        let node_id = self.nodes.len() as u16;
        self.nodes.push(RenderGraphNode {
            next: 0,
            first_child: 0,
            last_child: 0,
            command: RenderGraphCommand::DrawImmediate {
                first_index: first_index as u16,
                num_indices: indices.len() as u16,
            },
        });

        let parent = &mut self.nodes[parent.index as usize];
        let prev_sibling = parent.last_child as usize;
        parent.last_child = node_id;

        if parent.first_child == 0 {
            parent.first_child = node_id;
        } else {
            self.nodes[prev_sibling].next = node_id;
        }
    }

    pub fn draw_rect(
        &mut self,
        parent: RenderGraphNodeId,
        rect: Rect<f32>,
        colors: [Color; 4],
        corner_radii: Option<[f32; 4]>,
    ) {
        let rect_center = rect.center();
        let outer_radii = corner_radii.unwrap_or_default();
        let inner_radii = [0.0; 4];

        let vertices = [
            RoundedRectVertex {
                position: rect.top_left(),
                rect_size: rect.extent(),
                rect_center,
                outer_radii,
                inner_radii,
                color: colors[0],
            },
            RoundedRectVertex {
                position: rect.top_right(),
                rect_size: rect.extent(),
                rect_center,
                outer_radii,
                inner_radii,
                color: colors[1],
            },
            RoundedRectVertex {
                position: rect.bottom_right(),
                rect_size: rect.extent(),
                rect_center,
                outer_radii,
                inner_radii,
                color: colors[2],
            },
            RoundedRectVertex {
                position: rect.bottom_left(),
                rect_size: rect.extent(),
                rect_center,
                outer_radii,
                inner_radii,
                color: colors[3],
            },
        ];

        let vertex_offset = self.imm_rect_vertices.len();
        self.imm_rect_vertices.extend_from_slice(&vertices);

        let indices = [
            vertex_offset as u16,
            vertex_offset as u16 + 1,
            vertex_offset as u16 + 2,
            vertex_offset as u16,
            vertex_offset as u16 + 2,
            vertex_offset as u16 + 3,
        ];

        let first_index = self.imm_indices.len();
        self.imm_indices.extend_from_slice(&indices);

        let node_id = self.nodes.len() as u16;
        self.nodes.push(RenderGraphNode {
            next: 0,
            first_child: 0,
            last_child: 0,
            command: RenderGraphCommand::DrawRect {
                first_index: first_index as u16,
                num_indices: indices.len() as u16,
            },
        });

        let parent = &mut self.nodes[parent.index as usize];
        let prev_sibling = parent.last_child as usize;
        parent.last_child = node_id;

        if parent.first_child == 0 {
            parent.first_child = node_id;
        } else {
            self.nodes[prev_sibling].next = node_id;
        }
    }
}
