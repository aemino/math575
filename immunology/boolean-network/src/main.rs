mod model;

use std::{
    ops::{DerefMut, Range},
    time::Duration,
};

use bevy::prelude::*;
use bevy_fly_camera::{FlyCamera, FlyCameraPlugin};

use model::{cycle::CycleFinder, *};
use petgraph::visit::{EdgeRef, IntoNodeReferences};
use rand::{rngs::OsRng, Rng};

struct SimUpdateTimer(Timer);

struct ModelState {
    pub display_model: Model,
    pub compute_model: Model,
    pub cycle_finder: CycleFinder<u64>,
    pub cycle: Option<Range<usize>>,
}

struct SimNode {
    pub graph_id: u32,
    pub active: bool,
}

struct SimEdge {
    pub graph_id: u32,
}

struct CycleText;
struct PValueText;

struct MeshHandles {
    pub bulb: Handle<Mesh>,
    pub bulb_gate_indicator: Handle<Mesh>,
    pub wire_director: Handle<Mesh>,
}

struct MaterialHandles {
    pub bulb_inactive: Handle<StandardMaterial>,
    pub bulb_active: Handle<StandardMaterial>,
    pub gate_and: Handle<StandardMaterial>,
    pub gate_or: Handle<StandardMaterial>,
    pub gate_nor: Handle<StandardMaterial>,
    pub wire: Handle<StandardMaterial>,
}

struct ButtonMaterials {
    normal: Handle<ColorMaterial>,
    hovered: Handle<ColorMaterial>,
    pressed: Handle<ColorMaterial>,
}

impl FromWorld for ButtonMaterials {
    fn from_world(world: &mut World) -> Self {
        let mut materials = world.get_resource_mut::<Assets<ColorMaterial>>().unwrap();
        ButtonMaterials {
            normal: materials.add(Color::rgb(0.15, 0.15, 0.15).into()),
            hovered: materials.add(Color::rgb(0.25, 0.25, 0.25).into()),
            pressed: materials.add(Color::rgb(0.35, 0.75, 0.35).into()),
        }
    }
}

struct RegenerateButton;

struct RegenerateEvent;

const BULB_MESH_RADIUS: f32 = 1.0;
const WIRE_MESH_RADIUS_RATIO: f32 = 0.05;

fn main() {
    let mut app = App::build();

    app.insert_resource(Msaa { samples: 4 })
        .add_plugins(DefaultPlugins)
        .add_event::<RegenerateEvent>()
        .init_resource::<ButtonMaterials>()
        .add_startup_system(setup.system())
        .add_plugin(FlyCameraPlugin)
        .insert_resource(SimUpdateTimer(Timer::new(
            Duration::from_millis(1000),
            true,
        )))
        .add_system(generate_model.system())
        .add_system(update_model.system())
        .add_system(model_changed.system())
        .add_system(node_changed.system())
        .add_system(buttons.system())
        .add_system(regenerate_button.system());

    #[cfg(target_arch = "wasm32")]
    app.add_plugin(bevy_webgl2::WebGL2Plugin);

    #[cfg(target_arch = "wasm32")]
    app.add_plugin(bevy_web_fullscreen::FullViewportPlugin);

    app.run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    button_materials: Res<ButtonMaterials>,
    asset_server: Res<AssetServer>,
    mut regenerate_events: EventWriter<RegenerateEvent>,
) {
    // light
    commands.spawn_bundle(LightBundle {
        light: Light {
            range: 10000.0,
            // depth: 0.1..100.0,
            intensity: 5000.0,
            ..Default::default()
        },
        transform: Transform::from_translation(Vec3::new(0.0, 20.0, 0.0)),
        ..Default::default()
    });
    // camera
    commands
        .spawn()
        .insert_bundle(PerspectiveCameraBundle {
            transform: Transform::from_translation(Vec3::new(-2.0, 2.5, 5.0))
                .looking_at(Vec3::default(), Vec3::Y),
            ..Default::default()
        })
        .insert(FlyCamera::default());

    commands.spawn_bundle(UiCameraBundle::default());

    // asset handles
    commands.insert_resource(MeshHandles {
        bulb: meshes.add(
            shape::Icosphere {
                radius: BULB_MESH_RADIUS,
                subdivisions: 3,
            }
            .into(),
        ),
        bulb_gate_indicator: meshes.add(
            shape::Torus {
                radius: BULB_MESH_RADIUS * 0.25,
                ring_radius: BULB_MESH_RADIUS * 0.15,
                ..Default::default()
            }
            .into(),
        ),
        wire_director: meshes.add(
            shape::Torus {
                radius: (BULB_MESH_RADIUS * WIRE_MESH_RADIUS_RATIO) * 3.0,
                ring_radius: (BULB_MESH_RADIUS * WIRE_MESH_RADIUS_RATIO) * 2.0,
                ..Default::default()
            }
            .into(),
        ),
    });

    commands.insert_resource(MaterialHandles {
        bulb_inactive: materials.add(StandardMaterial {
            base_color: Color::rgba(0.8, 0.8, 0.95, 0.2),
            ..Default::default()
        }),
        bulb_active: materials.add(StandardMaterial {
            base_color: Color::rgba(1.0, 0.86, 0.25, 0.5),
            ..Default::default()
        }),
        gate_and: materials.add(Color::rgb(0.22, 0.95, 0.0).into()),
        gate_or: materials.add(Color::rgb(0.0, 0.68, 0.95).into()),
        gate_nor: materials.add(Color::rgb(0.95, 0.0, 0.22).into()),
        wire: materials.add(StandardMaterial {
            base_color: Color::rgb(0.7, 0.7, 0.7),
            ..Default::default()
        }),
    });

    // sim model
    regenerate_events.send(RegenerateEvent);

    // ui elements
    commands
        .spawn_bundle(TextBundle {
            style: Style {
                align_self: AlignSelf::FlexEnd,
                margin: Rect {
                    right: Val::Auto,
                    ..Default::default()
                },
                ..Default::default()
            },
            text: Text {
                sections: vec![
                    TextSection {
                        value: "Cycle: ".to_string(),
                        style: TextStyle {
                            font: asset_server.load("fonts/FiraCode-Bold.ttf"),
                            font_size: 60.0,
                            color: Color::WHITE,
                        },
                    },
                    TextSection {
                        value: "".to_string(),
                        style: TextStyle {
                            font: asset_server.load("fonts/FiraCode-Medium.ttf"),
                            font_size: 60.0,
                            color: Color::GOLD,
                        },
                    },
                ],
                ..Default::default()
            },
            ..Default::default()
        })
        .insert(CycleText);

    commands
        .spawn_bundle(TextBundle {
            style: Style {
                align_self: AlignSelf::FlexEnd,
                ..Default::default()
            },
            text: Text {
                sections: vec![
                    TextSection {
                        value: "P ≈ ".to_string(),
                        style: TextStyle {
                            font: asset_server.load("fonts/FiraCode-Bold.ttf"),
                            font_size: 60.0,
                            color: Color::WHITE,
                        },
                    },
                    TextSection {
                        value: "".to_string(),
                        style: TextStyle {
                            font: asset_server.load("fonts/FiraCode-Medium.ttf"),
                            font_size: 60.0,
                            color: Color::GOLD,
                        },
                    },
                ],
                ..Default::default()
            },
            ..Default::default()
        })
        .insert(PValueText);

    commands
        .spawn_bundle(ButtonBundle {
            style: Style {
                size: Size::new(Val::Px(300.0), Val::Px(65.0)),
                position_type: PositionType::Absolute,
                position: Rect {
                    left: Val::Percent(0.0),
                    right: Val::Auto,
                    top: Val::Auto,
                    bottom: Val::Percent(0.0),
                },
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..Default::default()
            },
            material: button_materials.normal.clone(),
            ..Default::default()
        })
        .with_children(|parent| {
            parent.spawn_bundle(TextBundle {
                text: Text::with_section(
                    "Regenerate Model",
                    TextStyle {
                        font: asset_server.load("fonts/NotoSans-Bold.ttf"),
                        font_size: 40.0,
                        color: Color::rgb(0.9, 0.9, 0.9),
                    },
                    Default::default(),
                ),
                ..Default::default()
            });
        })
        .insert(RegenerateButton);
}

fn generate_model(mut commands: Commands, mut events: EventReader<RegenerateEvent>) {
    if events.iter().count() == 0 {
        return;
    }

    const MIN_DIST: f32 = 3.0;
    const MAX_CONNECT_DIST: f32 = 5.0;
    const ACTIVE_PROB: f64 = 0.5;

    let gen_count = 200;
    let gen_radius = (gen_count as f32).sqrt() * 2.0;

    let mut model = Model::new();

    let mut rng = OsRng;

    'outer: while model.graph.node_count() < gen_count {
        let radius = rng.gen_range(0.0..gen_radius);
        let theta = rng.gen_range(0.0..std::f32::consts::TAU);

        let pos = Vec3::new(theta.cos() * radius, 0.0, theta.sin() * radius);

        let mut edges = Vec::new();
        for (node, weight) in model.graph.node_references() {
            let dist = weight.position.distance(pos);

            if dist < MIN_DIST {
                continue 'outer;
            }

            if dist > MAX_CONNECT_DIST {
                continue;
            }

            edges.push(node);
        }

        let node = model.graph.add_node(NodeWeight {
            kind: match rng.gen_range(0..3) {
                0 => NodeKind::And(rng.gen_bool(ACTIVE_PROB)),
                1 => NodeKind::Or(rng.gen_bool(ACTIVE_PROB)),
                2 => NodeKind::Nor(rng.gen_bool(ACTIVE_PROB)),
                _ => unreachable!(),
            },
            position: pos,
        });

        for other in edges {
            if rng.gen_bool(0.5) {
                model.graph.add_edge(node, other, ());
            } else {
                model.graph.add_edge(other, node, ());
            }
        }
    }

    commands.insert_resource(ModelState {
        display_model: model.clone(),
        compute_model: model,
        cycle_finder: CycleFinder::new(),
        cycle: Default::default(),
    });
}

fn update_model(
    time: Res<Time>,
    mut timer: ResMut<SimUpdateTimer>,
    model_opt: Option<ResMut<ModelState>>,
    mut nodes: Query<(Entity, &mut SimNode)>,
    mut cycle_text: Query<&mut Text, (With<CycleText>, Without<PValueText>)>,
    mut pvalue_text: Query<&mut Text, (With<PValueText>, Without<CycleText>)>,
) {
    let mut model = if let Some(model) = model_opt {
        model
    } else {
        return;
    };

    let ModelState {
        compute_model,
        display_model,
        cycle_finder,
        cycle,
    } = model.deref_mut();

    let state_hash = compute_model.step();

    if cycle.is_none() {
        *cycle = cycle_finder.check_next(&compute_model.state_hashes.as_slice(), state_hash);
    }

    for mut text in cycle_text.iter_mut() {
        text.sections[1].value = if let Some(cycle_range) = cycle {
            format!("μ = {}, λ = {}", cycle_range.start, cycle_range.len())
        } else {
            format!("searching (steps = {})", compute_model.timestep)
        }
    }

    for mut text in pvalue_text.iter_mut() {
        let latest_pvals = compute_model.p_values.iter().take(100);
        let pval_avg = (latest_pvals.len() as f32).recip() * latest_pvals.sum::<f32>();

        text.sections[1].value = format!("{:.2}", pval_avg);
    }

    if timer.0.tick(time.delta()).just_finished() {
        display_model.step();

        for (_, mut state) in nodes.iter_mut() {
            state.active = display_model
                .graph
                .node_weight(state.graph_id.into())
                .unwrap()
                .kind
                .state();
        }
    }
}

fn model_changed(
    mut commands: Commands,
    model_opt: Option<Res<ModelState>>,
    nodes: Query<Entity, With<SimNode>>,
    edges: Query<(Entity, &Handle<Mesh>), With<SimEdge>>,
    mesh_handles: Res<MeshHandles>,
    material_handles: Res<MaterialHandles>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let model = if let Some(model) = model_opt {
        model
    } else {
        return;
    };

    if !model.is_changed() {
        return;
    }

    for entity in nodes.iter() {
        commands.entity(entity).despawn_recursive();
    }

    for (entity, mesh_handle) in edges.iter() {
        commands.entity(entity).despawn_recursive();

        meshes.remove(mesh_handle).unwrap();
    }

    let display_model = &model.display_model;

    for (node_id, weight) in display_model.graph.node_references() {
        let graph_id = node_id.index() as u32;
        let active = weight.kind.state();

        commands
            .spawn()
            .insert(SimNode { graph_id, active })
            .insert_bundle(PbrBundle {
                mesh: mesh_handles.bulb.clone(),
                material: if active {
                    material_handles.bulb_active.clone()
                } else {
                    material_handles.bulb_inactive.clone()
                },
                transform: Transform::from_translation(weight.position),
                ..Default::default()
            })
            .with_children(|parent| {
                parent.spawn_bundle(PbrBundle {
                    mesh: mesh_handles.bulb_gate_indicator.clone(),
                    material: match weight.kind {
                        NodeKind::And(_) => material_handles.gate_and.clone(),
                        NodeKind::Or(_) => material_handles.gate_or.clone(),
                        NodeKind::Nor(_) => material_handles.gate_nor.clone(),
                    },
                    transform: Transform::from_translation(Vec3::Y * BULB_MESH_RADIUS * 1.1),
                    ..Default::default()
                });
            });
    }

    for edge in display_model.graph.edge_references() {
        let graph_id = edge.id().index() as u32;
        let (source_weight, target_weight) = (
            display_model.graph.node_weight(edge.source()).unwrap(),
            display_model.graph.node_weight(edge.target()).unwrap(),
        );

        let (source_pos, target_pos) = (source_weight.position, target_weight.position);
        let len = source_pos.distance(target_pos) - (BULB_MESH_RADIUS * 2.0);
        let midpoint = (source_pos + target_pos) * 0.5;
        let dir_vec = target_pos - source_pos;

        let mesh = shape::Capsule {
            radius: BULB_MESH_RADIUS * WIRE_MESH_RADIUS_RATIO,
            depth: len,
            ..Default::default()
        };

        let rotation = Quat::from_rotation_arc_colinear(Vec3::Y, dir_vec.normalize());

        let mut wire_transform = Transform::from_translation(midpoint);
        wire_transform.rotate(rotation);

        let director_transform = Transform::from_translation(Vec3::Y * len * 0.45);

        let director = commands
            .spawn()
            .insert_bundle(PbrBundle {
                mesh: mesh_handles.wire_director.clone(),
                material: material_handles.wire.clone(),
                transform: director_transform,
                ..Default::default()
            })
            .id();

        commands
            .spawn()
            .insert(SimEdge { graph_id })
            .insert_bundle(PbrBundle {
                mesh: meshes.add(mesh.into()),
                material: material_handles.wire.clone(),
                transform: wire_transform,
                ..Default::default()
            })
            .push_children(&[director]);
    }
}

fn node_changed(
    mut nodes: Query<(&SimNode, &mut Handle<StandardMaterial>)>,
    materials: Res<MaterialHandles>,
) {
    for (state, mut material) in nodes.iter_mut() {
        *material = if state.active {
            materials.bulb_active.clone()
        } else {
            materials.bulb_inactive.clone()
        };
    }
}

fn buttons(
    button_materials: Res<ButtonMaterials>,
    mut interaction_query: Query<
        (&Interaction, &mut Handle<ColorMaterial>),
        (Changed<Interaction>, With<Button>),
    >,
) {
    for (interaction, mut material) in interaction_query.iter_mut() {
        match *interaction {
            Interaction::Clicked => {
                *material = button_materials.pressed.clone();
            }
            Interaction::Hovered => {
                *material = button_materials.hovered.clone();
            }
            Interaction::None => {
                *material = button_materials.normal.clone();
            }
        }
    }
}

fn regenerate_button(
    interactions: Query<&Interaction, (Changed<Interaction>, With<Button>, With<RegenerateButton>)>,
    mut events: EventWriter<RegenerateEvent>,
) {
    for interaction in interactions.iter() {
        if let Interaction::Clicked = interaction {
            events.send(RegenerateEvent);
        }
    }
}
