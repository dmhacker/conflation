use conflation::sync::mpmc::unbounded;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::sync::mpsc::channel;
use std::thread;

fn standard_n_unique(n: u64) {
    let (tx, rx) = channel();
    let handle = thread::spawn(move || loop {
        match rx.recv() {
            Err(_) => break,
            _ => (),
        }
    });
    for i in 0..n {
        tx.send((i, "some string".to_owned())).unwrap();
    }
    drop(tx);
    handle.join().unwrap();
}

fn standard_n_duplicates(n: u64) {
    let (tx, rx) = channel();
    let handle = thread::spawn(move || loop {
        match rx.recv() {
            Err(_) => break,
            _ => (),
        }
    });
    for _ in 0..n {
        tx.send((0, "some string".to_owned())).unwrap();
    }
    drop(tx);
    handle.join().unwrap();
}

fn conflated_n_unique(n: u64) {
    let (tx, rx) = unbounded();
    let handle = thread::spawn(move || loop {
        match rx.recv() {
            Err(_) => break,
            _ => (),
        }
    });
    for i in 0..n {
        tx.send(i, "some string".to_owned()).unwrap();
    }
    drop(tx);
    handle.join().unwrap();
}

fn conflated_n_duplicates(n: u64) {
    let (tx, rx) = unbounded();
    let handle = thread::spawn(move || loop {
        match rx.recv() {
            Err(_) => break,
            _ => (),
        }
    });
    for _ in 0..n {
        tx.send(0, "some string".to_owned()).unwrap();
    }
    drop(tx);
    handle.join().unwrap();
}

pub fn standard_bench(c: &mut Criterion) {
    for i in 2..5 {
        let n = 10_u64.pow(i);
        let title = format!("standard | {} unique", &n);
        c.bench_function(&title[..], |b| {
            b.iter(|| standard_n_unique(black_box(n)))
        });
    }
    for i in 2..5 {
        let n = 10_u64.pow(i);
        let title = format!("standard | {} duplicates", &n);
        c.bench_function(&title[..], |b| {
            b.iter(|| standard_n_duplicates(black_box(n)))
        });
    }
}

pub fn conflated_bench(c: &mut Criterion) {
    for i in 2..5 {
        let n = 10_u64.pow(i);
        let title = format!("conflated | {} unique", &n);
        c.bench_function(&title[..], |b| {
            b.iter(|| conflated_n_unique(black_box(n)))
        });
    }
    for i in 2..5 {
        let n = 10_u64.pow(i);
        let title = format!("conflated | {} duplicates", &n);
        c.bench_function(&title[..], |b| {
            b.iter(|| conflated_n_duplicates(black_box(n)))
        });
    }
}

criterion_group!(benches, standard_bench, conflated_bench);
criterion_main!(benches);
