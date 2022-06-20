use conflation::sync::mpmc::unbounded;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::sync::mpsc::channel;
use std::thread;

fn unique_items_in_conflated_queue(n: u64) {
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

fn unique_items_in_standard_queue(n: u64) {
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

fn duplicate_items_in_conflated_queue(n: u64) {
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

fn duplicate_items_in_standard_queue(n: u64) {
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

pub fn standard_queue(c: &mut Criterion) {
    for i in 2..5 {
        let n = 10_u64.pow(i);
        let title = format!("unique // standard // {}", &n);
        c.bench_function(&title[..], |b| {
            b.iter(|| unique_items_in_standard_queue(black_box(n)))
        });
    }
    for i in 2..5 {
        let n = 10_u64.pow(i);
        let title = format!("duplicate // standard // {}", &n);
        c.bench_function(&title[..], |b| {
            b.iter(|| duplicate_items_in_standard_queue(black_box(n)))
        });
    }
}

pub fn conflated_queue(c: &mut Criterion) {
    for i in 2..5 {
        let n = 10_u64.pow(i);
        let title = format!("unique // conflated // {}", &n);
        c.bench_function(&title[..], |b| {
            b.iter(|| unique_items_in_conflated_queue(black_box(n)))
        });
    }
    for i in 2..5 {
        let n = 10_u64.pow(i);
        let title = format!("duplicate // conflated // {}", &n);
        c.bench_function(&title[..], |b| {
            b.iter(|| duplicate_items_in_conflated_queue(black_box(n)))
        });
    }
}

criterion_group!(benches, standard_queue, conflated_queue);
criterion_main!(benches);
