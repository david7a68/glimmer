use geometry::Point;

#[derive(Clone, Copy)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

#[derive(Clone, Copy)]
pub struct Vertex {
    pub position: Point<f32>,
    pub color: Color,
}

#[repr(u16)]
enum RenderGraphCommand {
    Root,
    DrawImmediate { first_index: u16, num_indices: u16 },
}

struct RenderGraphNode {
    next: u16,
    first_child: u16,
    last_child: u16,
    command: RenderGraphCommand,
}

pub struct RenderGraphNodeId {
    index: u16,
}

impl RenderGraphNodeId {
    pub const fn root() -> Self {
        Self { index: 0 }
    }
}

pub struct RenderGraph {
    imm_indices: Vec<u16>,
    imm_vertices: Vec<Vertex>,
    nodes: Vec<RenderGraphNode>,
}

impl Default for RenderGraph {
    fn default() -> Self {
        Self {
            imm_indices: Vec::new(),
            imm_vertices: Vec::new(),
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
    pub fn new() -> Self {
        Self::default()
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
    pub fn draw_immediate(
        &mut self,
        parent: RenderGraphNodeId,
        vertices: &[Vertex],
        indices: &[u16],
    ) {
        let vertex_offset = self.imm_vertices.len();
        self.imm_vertices.extend_from_slice(vertices);

        let first_index = self.imm_vertices.len();
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
        parent.last_child = node_id as u16;

        if parent.first_child == 0 {
            parent.first_child = node_id as u16;
        } else {
            self.nodes[prev_sibling].next = node_id as u16;
        }
    }
}
