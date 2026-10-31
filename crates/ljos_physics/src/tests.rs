use super::*;

fn world(length: f32) -> World {
    World::new(
        Tuning {
            length,
            ccd: true,
            ..Tuning::default()
        },
        [0.0, -980.7, 0.0],
    )
}

fn part(shape: Shape) -> Part {
    Part {
        shape,
        pose: Pose::default(),
    }
}

fn at(y: f32) -> Pose {
    Pose {
        position: [0.0, y, 0.0],
        ..Pose::default()
    }
}

fn floor(w: &mut World, y: f32) -> BodyId {
    // A plane faces +Z; a quarter turn about X makes that up.
    let s = std::f32::consts::FRAC_1_SQRT_2;
    let f = w.add_body(
        Kind::Static,
        Pose {
            position: [0.0, y, 0.0],
            rotation: [s, -s, 0.0, 0.0],
        },
    );
    w.set_parts(f, vec![part(Shape::Plane)]);
    f
}

#[test]
fn a_body_falls_as_far_as_gravity_says() {
    let mut w = world(100.0);
    let b = w.add_body(Kind::Dynamic, at(0.0));
    w.set_parts(b, vec![part(Shape::Sphere { radius: 1.0 })]);
    for _ in 0..60 {
        w.step(1.0 / 60.0);
    }
    let y = w.pose(b).unwrap().position[1];
    // Half g t² from the steps' own integration, which runs a little ahead.
    assert!((-500.0..-480.0).contains(&y), "fell to {y}");
}

#[test]
fn a_force_lasts_one_step() {
    let mut w = world(100.0);
    w.set_gravity([0.0; 3]);
    let b = w.add_body(Kind::Dynamic, at(0.0));
    w.set_parts(b, vec![part(Shape::Sphere { radius: 1.0 })]);
    w.set_mass(b, Mass::Total(2.0));
    w.command(b, Command::Force([100.0, 0.0, 0.0]));
    w.step(0.01);
    let first = w.linear_velocity(b).unwrap()[0];
    assert!((first - 0.5).abs() < 1e-3, "velocity {first}");
    w.step(0.01);
    assert!((w.linear_velocity(b).unwrap()[0] - first).abs() < 1e-5);
}

#[test]
fn a_small_die_dropped_fast_lands_and_sleeps() {
    // Things a centimetre long measured in centimetres, at a real gravity,
    // with a long step: what tunnels or jitters if tolerances are wrong.
    let mut w = world(1.0);
    let f = floor(&mut w, 0.0);
    let die = w.add_body(Kind::Dynamic, at(300.0));
    w.set_parts(die, vec![part(Shape::Box { half: [1.0; 3] })]);
    w.set_mass(die, Mass::Density(1.13));
    let mut touched = 0;
    let mut steps = 0;
    while !w.is_sleeping(die) && steps < 2000 {
        let s = w.step(0.02);
        touched += s
            .contacts
            .iter()
            .filter(|c| (c.a == f && c.b == die) || (c.a == die && c.b == f))
            .count();
        steps += 1;
    }
    assert!(w.is_sleeping(die), "still moving after {steps} steps");
    assert!(touched >= 1);
    let y = w.pose(die).unwrap().position[1];
    assert!((0.9..1.8).contains(&y), "rests at {y}");
    assert!(!w.has_motion());
}

#[test]
fn a_touch_is_told_once_with_points_and_a_normal_towards_a() {
    let mut w = world(10.0);
    w.set_gravity([0.0, -100.0, 0.0]);
    let f = floor(&mut w, 0.0);
    let b = w.add_body(Kind::Dynamic, at(3.0));
    // Not bouncing, so that it touches only once.
    let dull = Material {
        restitution: 0.0,
        ..Material::default()
    };
    w.set_material(b, dull);
    w.set_material(f, dull);
    w.set_parts(b, vec![part(Shape::Box { half: [1.0; 3] })]);
    let mut told = Vec::new();
    for _ in 0..200 {
        told.extend(w.step(0.01).contacts);
    }
    assert_eq!(told.len(), 1, "{told:?}");
    let c = &told[0];
    assert!(!c.positions.is_empty());
    let up = if c.a == b { 1.0 } else { -1.0 };
    assert!(c.normals[0][1] * up > 0.9, "{c:?}");
    assert!(c.a == f || c.b == f);
}

#[test]
fn a_trigger_tells_of_what_comes_in_and_goes_out() {
    let mut w = world(10.0);
    w.set_gravity([0.0; 3]);
    let t = w.add_body(Kind::Trigger, at(0.0));
    w.set_parts(t, vec![part(Shape::Box { half: [1.0; 3] })]);
    let b = w.add_body(Kind::Dynamic, at(3.0));
    w.set_parts(b, vec![part(Shape::Sphere { radius: 0.5 })]);
    w.command(b, Command::LinearVelocity([0.0, -10.0, 0.0]));
    let mut told = Vec::new();
    for _ in 0..100 {
        told.extend(w.step(0.01).triggers);
    }
    assert_eq!(
        told,
        vec![
            TriggerChange {
                trigger: BodyId(t.0),
                other: b,
                entered: true
            },
            TriggerChange {
                trigger: BodyId(t.0),
                other: b,
                entered: false
            }
        ]
    );
}

#[test]
fn a_filter_lets_a_body_through() {
    let mut w = World::new(Tuning::default(), [0.0, -980.0, 0.0]);
    let f = floor(&mut w, 0.0);
    w.set_filter(
        f,
        Filter {
            group: 1,
            ignored: 0,
        },
    );
    w.set_parts(f, vec![part(Shape::Plane)]);
    let b = w.add_body(Kind::Dynamic, at(10.0));
    w.set_filter(
        b,
        Filter {
            group: 0,
            ignored: 1 << 1,
        },
    );
    w.set_parts(b, vec![part(Shape::Sphere { radius: 1.0 })]);
    for _ in 0..60 {
        w.step(1.0 / 60.0);
    }
    assert!(w.pose(b).unwrap().position[1] < -100.0);
}

#[test]
fn a_character_stands_on_the_floor_and_stops_at_a_wall() {
    let mut w = world(100.0);
    floor(&mut w, 0.0);
    let wall = w.add_body(
        Kind::Static,
        Pose {
            position: [50.0, 50.0, 0.0],
            ..Pose::default()
        },
    );
    w.set_parts(
        wall,
        vec![part(Shape::Box {
            half: [10.0, 50.0, 50.0],
        })],
    );
    w.step(0.01);
    let c = w.add_body(Kind::Kinematic, at(30.0));
    let mut last = Moved::default();
    for _ in 0..200 {
        last = w.move_character(c, (10.0, 10.0), 5.0, [1.0, -5.0, 0.0], 0.016);
        w.step(0.016);
    }
    let p = w.pose(c).unwrap().position;
    assert!(p[0] < 30.5 && p[0] > 20.0, "at {p:?}");
    assert!((19.0..21.5).contains(&p[1]), "at {p:?}");
    assert!(last.touches_down);
    assert!(last.touches_side);
}
