use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use roms_curator::core::roms_service::RomsExt;

mod utils;

fn copy_roms(c: &mut Criterion) {
    c.bench_function("copy_roms", |b| {
        utils::set_up("copy_roms_bench");
        b.iter_batched(
            || {
                let output = tempfile::tempdir().unwrap();
                let mut args =
                    utils::build_args("copy_roms_bench", false, String::new(), String::new());
                args.destination_path = output.path().join("roms").to_str().unwrap().into();
                args.report_path.clear();
                (output, args)
            },
            |(output, args)| {
                let results = black_box(roms_curator::run(&args).unwrap());
                let report = results.copy_roms(&args).expect("Error copying roms");
                black_box((output, report))
            },
            BatchSize::PerIteration,
        );
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(10);
    targets = copy_roms
}
criterion_main!(benches);
