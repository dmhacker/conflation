use conflation::sync::mpmc::unbounded;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::thread;

fn conflated_n_k_stress(n: u64, k: u64) {
    let (tx, rx) = unbounded();
    let mut consumers = vec![];
    for _ in 0..k {
        let rx_clone = rx.clone();
        consumers.push(thread::spawn(move || loop {
            match rx_clone.recv() {
                Err(_) => break,
                _ => (),
            }
        }));
    }
    let mut producers = vec![];
    for _ in 0..k {
        let tx_clone = tx.clone();
        producers.push(thread::spawn(move || {
            for i in 0..n {
                tx_clone.send(i % 10, "some string".to_owned()).unwrap();
            }
        }));
    }
    drop(tx);
    drop(rx);
    for handle in producers.drain(..) {
        handle.join().unwrap();
    }
    for handle in consumers.drain(..) {
        handle.join().unwrap();
    }
}

pub fn conflated_bench(c: &mut Criterion) {
    for n in vec![1000, 10000] {
        for k in vec![1, 2, 4, 6, 8] {
            let title = format!(
                "conflated | ({} items each, {} producers, {} consumers)",
                &n, &k, &k
            );
            c.bench_function(&title[..], |b| {
                b.iter(|| conflated_n_k_stress(black_box(n), black_box(k)))
            });
        }
    }
}

criterion_group!(benches, conflated_bench);
criterion_main!(benches);
