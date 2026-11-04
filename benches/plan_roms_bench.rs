use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};

mod utils;

fn plan_export(c: &mut Criterion) {
    let tag = "plan_roms_bench";
    utils::set_up(tag);
    let args = utils::build_args(tag, false);
    let catalog = utils::load_catalog(&args);
    let label = utils::bench_metadata_label();

    c.bench_function(&format!("plan_export/{label}"), |b| {
        b.iter(|| black_box(catalog.plan(&args).unwrap()));
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(10);
    targets = plan_export
}
criterion_main!(benches);
