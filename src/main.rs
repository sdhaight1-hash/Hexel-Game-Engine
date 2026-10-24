use hexel_engine::{BlockId, CellPos, World, WorldScale, generated_block};

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let seed = 0x4845_5845;
    let scale = WorldScale::default();
    let mut world = World::default();
    for q in -8..8 {
        for r in -8..8 {
            let top = hexel_engine::terrain_height(seed, q, r);
            for y in (top - 3)..=top {
                world.set(CellPos::new(q, r, y), generated_block(seed, CellPos::new(q, r, y)));
            }
        }
    }

    tracing::info!(
        seed,
        chunks = world.chunk_count(),
        player_height = scale.player_height,
        hexel_height = scale.hexel_height,
        "Hexel core initialized"
    );
    println!("Hexel Engine core initialized. Native interactive renderer/controller are not implemented yet.");
    println!("World: axial (q,r) + vertical y; chunks 16×16×16; seed {seed}.");
    let _ = BlockId::Grass;
}
