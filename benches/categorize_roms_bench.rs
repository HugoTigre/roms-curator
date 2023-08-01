use criterion::{black_box, Criterion, criterion_group, criterion_main};

mod utils;

fn categorize_roms(c: &mut Criterion) {
    c.bench_function("categorize_roms", |b| {
        let tag = "categorize_roms_bench";
        utils::set_up(tag);

        let args = utils::build_args(
            tag, false, String::new(), String::new(),
        );

        b.iter(|| {
            black_box(
                roms_curator::run(&args).unwrap()
            );
        });
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(10);
    targets = categorize_roms
}
criterion_main!(benches);
