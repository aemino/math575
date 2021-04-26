pub mod cycle;

use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use bevy::math::Vec3;
use petgraph::{graph::DiGraph, visit::IntoNodeReferences, EdgeDirection};

#[derive(Debug, Clone, Copy, Hash)]
pub enum NodeKind {
    And(bool),
    Or(bool),
    Nor(bool),
}

impl NodeKind {
    pub fn state(&self) -> bool {
        match self {
            NodeKind::And(state) => *state,
            NodeKind::Or(state) => *state,
            NodeKind::Nor(state) => *state,
        }
    }

    pub fn update(&self, inputs: impl Iterator<Item = bool>) -> Self {
        let mut peekable_inputs = inputs.peekable();

        match self {
            NodeKind::And(_) => {
                NodeKind::And(peekable_inputs.peek().is_some() && peekable_inputs.all(|x| x))
            }
            NodeKind::Or(_) => NodeKind::Or(peekable_inputs.any(|x| x)),
            NodeKind::Nor(_) => NodeKind::Nor(!peekable_inputs.any(|x| x)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct NodeWeight {
    pub kind: NodeKind,
    pub position: Vec3,
}

impl Hash for NodeWeight {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.kind.hash(state);
    }
}

#[derive(Clone)]
pub struct Model {
    pub timestep: usize,
    pub state_hashes: Vec<u64>,
    pub p_values: Vec<f32>,
    pub graph: DiGraph<NodeWeight, ()>,
}

impl Model {
    pub fn new() -> Self {
        Self {
            timestep: Default::default(),
            state_hashes: Default::default(),
            p_values: Default::default(),
            graph: Default::default(),
        }
    }

    fn push_state_hash(&mut self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.hash(&mut hasher);

        self.state_hashes.push(hasher.finish());
        return hasher.finish();
    }

    pub fn step(&mut self) -> u64 {
        if self.timestep == 0 {
            self.push_state_hash();
        }

        let new_weights = self
            .graph
            .node_references()
            .map(|(node, weight)| {
                let input_states = self
                    .graph
                    .neighbors_directed(node, EdgeDirection::Incoming)
                    .map(|adj| self.graph.node_weight(adj).unwrap().kind.state());

                NodeWeight {
                    kind: weight.kind.update(input_states),
                    ..weight.clone()
                }
            })
            .collect::<Vec<_>>();

        let states = new_weights.iter().map(|weight| weight.kind.state());
        let p_value = (states.len() as f32).recip() * states.filter(|&x| x).count() as f32;
        self.p_values.insert(0, p_value);

        self.graph
            .node_weights_mut()
            .zip(new_weights)
            .for_each(|(weight, new_weight)| {
                *weight = new_weight;
            });

        self.timestep += 1;

        self.push_state_hash()
    }
}

impl Hash for Model {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.graph
            .node_references()
            .for_each(|(_, weight)| weight.hash(state));
    }
}
