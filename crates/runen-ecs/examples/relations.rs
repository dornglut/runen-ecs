use runen_ecs::prelude::*;

#[derive(Debug, Component)]
struct Name(&'static str);

struct Owns;

impl Relation for Owns {
    type Kind = Directed;
}

struct AlliedWith;

impl Relation for AlliedWith {
    type Kind = Symmetric;
}

fn name(world: &World, entity: Entity) -> &'static str {
    world
        .require::<Name>(entity)
        .expect("named entity should remain live")
        .0
}

fn main() {
    let mut world = World::new();

    let alice = world.spawn(Name("Alice")).expect("Alice should spawn");
    let bob = world.spawn(Name("Bob")).expect("Bob should spawn");
    let sword = world.spawn(Name("Sword")).expect("Sword should spawn");
    let shield = world.spawn(Name("Shield")).expect("Shield should spawn");

    {
        let mut owns = world.relations_mut::<Owns>();
        // Insert opposite to Entity order: relation observations are deterministic,
        // but they do not promise insertion order.
        assert!(owns.insert(alice, shield).unwrap());
        assert!(owns.insert(alice, sword).unwrap());
    }

    {
        let mut allies = world.relations_mut::<AlliedWith>();
        assert!(allies.insert(bob, alice).unwrap());
    }

    {
        let owns = world.relations::<Owns>();
        assert!(owns.contains(alice, sword));
        assert!(!owns.contains(sword, alice));

        let owned = owns
            .targets(alice)
            .unwrap()
            .iter()
            .collect::<Vec<_>>();
        assert_eq!(owned, vec![sword, shield]);

        let sword_owners = owns
            .sources(sword)
            .unwrap()
            .iter()
            .collect::<Vec<_>>();
        assert_eq!(sword_owners, vec![alice]);

        let allies = world.relations::<AlliedWith>();
        assert!(allies.contains(alice, bob));
        assert!(allies.contains(bob, alice));

        let alice_allies = allies
            .neighbors(alice)
            .unwrap()
            .iter()
            .collect::<Vec<_>>();
        assert_eq!(alice_allies, vec![bob]);

        println!(
            "{} owns {} and {}; {} is allied with {}",
            name(&world, alice),
            name(&world, owned[0]),
            name(&world, owned[1]),
            name(&world, alice),
            name(&world, alice_allies[0]),
        );
    }

    world.despawn(alice).expect("Alice should despawn");

    assert!(world.relations::<Owns>().is_empty());
    assert!(world.relations::<AlliedWith>().is_empty());
    assert!(world.contains(bob));
    assert!(world.contains(sword));
    assert!(world.contains(shield));

    println!("despawning Alice removed all of her incident relation edges");
}
