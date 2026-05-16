use btree::BTree;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use tempfile::NamedTempFile;

fn bench_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("insert");

    for size in [1_000u64, 10_000, 100_000] {
        group.throughput(Throughput::Elements(size));
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, &n| {
            b.iter(|| {
                let file = NamedTempFile::new().unwrap();
                let mut tree: BTree<String, u64> = BTree::open(file.path(), 50).unwrap();
                for i in 0..n {
                    tree.insert(format!("{:016}", i), i).unwrap();
                }
            });
        });
    }
    group.finish();
}

fn bench_get(c: &mut Criterion) {
    let file = NamedTempFile::new().unwrap();
    let mut tree: BTree<String, u64> = BTree::open(file.path(), 50).unwrap();
    for i in 0u64..10_000 {
        tree.insert(format!("{:016}", i), i).unwrap();
    }

    let mut group = c.benchmark_group("get");
    group.throughput(Throughput::Elements(1));
    group.bench_function("random_key", |b| {
        let mut x: u64 = 0xcafebabe;
        b.iter(|| {
            x = x.wrapping_mul(6364136223846793005).wrapping_add(1);
            let k = format!("{:016}", x % 10_000);
            let _ = tree.get(&k);
        });
    });
    group.finish();
}

fn bench_scan(c: &mut Criterion) {
    let file = NamedTempFile::new().unwrap();
    let mut tree: BTree<String, u64> = BTree::open(file.path(), 50).unwrap();
    for i in 0u64..10_000 {
        tree.insert(format!("{:016}", i), i).unwrap();
    }

    let mut group = c.benchmark_group("scan");
    group.throughput(Throughput::Elements(10_000));
    group.bench_function("full_scan", |b| {
        b.iter(|| {
            let _ = tree.scan_all().unwrap();
        });
    });
    group.finish();
}

criterion_group!(benches, bench_insert, bench_get, bench_scan);
criterion_main!(benches);