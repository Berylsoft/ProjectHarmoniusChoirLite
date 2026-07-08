#![allow(clippy::type_complexity)]

use std::collections::HashSet;

use bevy_ecs::{
    component::Component,
    entity::Entity,
    query::Without,
    schedule::{IntoScheduleConfigs, Schedule},
    system::{Commands, Query},
    world::World,
};
use itertools::Itertools;

#[derive(Debug, Component)]
pub struct User {
    pub id: i64,
}

#[derive(Debug, Component)]
pub struct File {
    pub name: Box<str>,
    pub nth: u64,
}

#[derive(Debug, Component)]
pub struct Rejected;

#[derive(Debug, Component)]
pub struct Passed {
    groups: HashSet<Group>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Group {
    A,
    B,
    C,
    D,
}

#[derive(Debug, Component)]
pub struct Replaced;

#[derive(Debug, Component)]
pub struct SubmitFile {
    uid: i64,
    file_name: Box<str>,
}

#[derive(Debug, Component)]
pub struct PassWith {
    groups: HashSet<Group>,
}

type PendingFilter = (
    Without<Rejected>,
    Without<Passed>,
    Without<PassWith>,
    Without<Replaced>,
);

pub fn process_submit_system(
    world: &World,
    mut commands: Commands,
    submits: Query<(Entity, &SubmitFile)>,
    exists_pending: Query<(Entity, &User), PendingFilter>,
    exists: Query<(&User, &File)>,
) {
    if let Some((e, submit)) = submits.iter().next() {
        commands.entity(e).despawn();
        println!("{submit:?}");

        if let Some((pending_e, _)) = exists_pending
            .iter()
            .filter(|(_, u)| u.id == submit.uid)
            .at_most_one()
            .expect("at most one")
        {
            println!(
                "  replace last pending: {:?}",
                world.entity(pending_e).components::<&File>()
            );
            commands.entity(pending_e).insert(Replaced);
        }

        let exists = exists
            .iter()
            .filter(|(u, ..)| u.id == submit.uid)
            .map(|(_, f, ..)| f.nth)
            .max();
        let nth = if let Some(nth) = exists { nth + 1 } else { 1 };
        commands.spawn((
            User { id: submit.uid },
            File {
                name: submit.file_name.clone(),
                nth,
            },
        ));
    }
}

pub fn review_system(
    mut commands: Commands,
    pending: Query<(Entity, &User, &File), PendingFilter>,
) {
    for (e, u, f) in pending {
        println!("review {u:?} {f:?}");
        match &*f.name {
            "a submit.wav" => {
                println!("  reject");
                commands.entity(e).insert(Rejected);
            }
            "b submit.wav" => {
                println!("  pass with {{A}}");
                commands.entity(e).insert(PassWith {
                    groups: [Group::A].into(),
                });
            }
            "c submit.wav" => {
                println!("  pass with {{A}}");
                commands.entity(e).insert(PassWith {
                    groups: [Group::A].into(),
                });
            }
            "d submit.wav" => {
                println!("  pass with {{A, B}}");
                commands.entity(e).insert(PassWith {
                    groups: [Group::A, Group::B].into(),
                });
            }
            _ => {
                println!("  skip");
            }
        }
    }
}

pub fn handle_pass_system(
    mut commands: Commands,
    passes: Query<(Entity, &User, &PassWith)>,
    last_pass: Query<(Entity, &User, &Passed), Without<Replaced>>,
) {
    for (e, u, pass) in passes {
        println!("apply pass: {e:?} {u:?} {pass:?}");
        commands.entity(e).remove::<PassWith>();
        commands.entity(e).insert(Passed {
            groups: pass.groups.clone(),
        });

        let last_pass = last_pass
            .iter()
            .filter(|(_, it, ..)| it.id == u.id)
            .at_most_one()
            .expect("at most one");
        if let Some((last_e, _, last_pass)) = last_pass {
            if pass.groups.is_superset(&last_pass.groups) {
                println!("  replace last one");
                commands.entity(last_e).insert(Replaced);
            } else {
                println!("  ignore current one");
                commands.entity(e).insert(Replaced);
            }
        }
    }
}

fn main() {
    let mut world = World::new();
    let mut schedule = Schedule::default();
    schedule.add_systems((
        process_submit_system,
        review_system.after(process_submit_system),
        handle_pass_system.after(review_system),
    ));

    world.spawn(SubmitFile {
        uid: 1,
        file_name: "_ submit.wav".into(),
    });
    world.spawn(SubmitFile {
        uid: 1,
        file_name: "a submit.wav".into(),
    });
    world.spawn(SubmitFile {
        uid: 2,
        file_name: "b submit.wav".into(),
    });

    dump_world(&mut world);
    schedule.run(&mut world);
    dump_world(&mut world);
    schedule.run(&mut world);
    dump_world(&mut world);
    schedule.run(&mut world);
    dump_world(&mut world);

    world.spawn(SubmitFile {
        uid: 1,
        file_name: "b submit.wav".into(),
    });

    schedule.run(&mut world);
    dump_world(&mut world);

    world.spawn(SubmitFile {
        uid: 1,
        file_name: "c submit.wav".into(),
    });
    world.spawn(SubmitFile {
        uid: 1,
        file_name: "d submit.wav".into(),
    });

    schedule.run(&mut world);
    dump_world(&mut world);
    schedule.run(&mut world);
    dump_world(&mut world);
}

fn dump_world(world: &mut World) {
    println!();
    println!("dumping world:");
    for e in world.query::<Entity>().iter(world) {
        println!("  {}v{}", e.index_u32(), e.generation().to_bits());
        let e = world.entity(e);
        if let Ok((u, f)) = e.get_components::<(&User, &File)>() {
            println!("    {u:?} {f:?}");
        }
        if let Ok(it) = e.get_components::<&Passed>() {
            println!("    {it:?}");
        }
        if let Ok(it) = e.get_components::<&SubmitFile>() {
            println!("    {it:?}");
        }

        for c in e.archetype().components() {
            let it = world.component_id::<User>();
            if Some(*c) == it {
                continue;
            }
            let it = world.component_id::<File>();
            if Some(*c) == it {
                continue;
            }
            let it = world.component_id::<Passed>();
            if Some(*c) == it {
                continue;
            }
            let it = world.component_id::<SubmitFile>();
            if Some(*c) == it {
                continue;
            }

            println!(
                "    {:?}",
                world.components().get_info(*c).unwrap().name()
            );
        }
    }

    println!();
}
