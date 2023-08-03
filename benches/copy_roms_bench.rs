use criterion::{black_box, Criterion, criterion_group, criterion_main};
use roms_curator::core::roms_service::RomsExt;

mod utils;

fn copy_roms(c: &mut Criterion) {
    c.bench_function("copy_roms", |b| {
        let tag = "copy_roms_bench";
        utils::set_up(tag);

        let args = utils::build_args(
            tag, false, String::new(), String::new(),
        );

        b.iter(|| {
            let results = black_box(
                roms_curator::run(&args).unwrap()
            );
            black_box(
                results.copy_roms(&args).expect("Error copying roms")
            );
        });
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(10);
    targets = copy_roms
}
criterion_main!(benches);
