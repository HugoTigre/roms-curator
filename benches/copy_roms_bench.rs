use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};

mod utils;

fn copy_roms(c: &mut Criterion) {
    let tag = "copy_roms_bench";
    utils::set_up(tag);
    let base_args = utils::build_args(tag, false);
    let label = utils::bench_metadata_label();
    let source_files = utils::count_source_files();

    c.bench_function(&format!("copy_roms/{label}/sources={source_files}"), |b| {
        b.iter_batched(
            || {
                let output = tempfile::tempdir().expect("temp export directory");
                let mut args = base_args.clone();
                args.destination_path = utils::export_destination(output.path())
                    .to_string_lossy()
                    .into_owned();
                let catalog = utils::load_catalog(&args);
                let plan = catalog.plan(&args).expect("build plan for benchmark");
                (output, catalog, plan, args)
            },
            |(output, catalog, plan, args)| {
                let report = catalog
                    .copy_with_plan(&args, &plan, None)
                    .expect("copy roms in benchmark");
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
