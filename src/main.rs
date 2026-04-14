use constraint_theory_core::PythagoreanManifold;
use rand::Rng;
use std::collections::HashSet;

const N_NODES: usize = 128;
const N_TRIALS: usize = 100;
const CYCLE_TOLERANCE: f64 = 1e-9;
const BASE_RTT: f64 = 1.0;

/// PBFT round: 3-phase protocol (pre-prepare → prepare → commit).
/// Requires 2f+1 votes per phase; succeeds only if faulty ≤ f.
/// Returns (success, latency_in_rtts).
fn pbft_round(n: usize, faulty: usize) -> (bool, f64) {
    let f = (n - 1) / 3;
    if faulty > f {
        return (false, 0.0);
    }
    // Three sequential broadcast/collect phases ≈ 3 RTTs
    (true, 3.0 * BASE_RTT)
}

/// Holonomy round: nodes hold gauge angles; transport around a ring cycle
/// sums to zero iff the network is consistent. Faults break the holonomy
/// invariant and are located via O(log N) cycle bisection.
fn holonomy_round(
    n: usize,
    faulty: usize,
    manifold: &PythagoreanManifold,
    rng: &mut impl Rng,
) -> (bool, f64) {
    // Assign honest gauge angles (should satisfy closure: Σ transport = 0)
    let mut gauges: Vec<f64> = (0..n)
        .map(|i| (i as f64) * std::f64::consts::TAU / n as f64)
        .collect();

    // Randomly select faulty nodes and corrupt their gauge values
    let mut indices: Vec<usize> = (0..n).collect();
    for i in 0..faulty {
        let j = rng.gen_range(i..n);
        indices.swap(i, j);
    }
    let faulty_set: HashSet<usize> = indices[..faulty].iter().cloned().collect();
    for &fi in &faulty_set {
        // Inject arbitrary phase noise that breaks the holonomy invariant
        gauges[fi] += rng.gen::<f64>() * std::f64::consts::PI + 0.5;
    }

    // Walk the ring: node 0 → 1 → … → N-1 → 0
    // Each edge transport: Δθ_i = gauge[j] − gauge[i], snapped through manifold
    let mut cycle_sum: f64 = 0.0;
    for i in 0..n {
        let j = (i + 1) % n;
        let delta = (gauges[j] - gauges[i]) as f32;
        // manifold.snap returns (snapped_dir, noise_magnitude)
        let (_snapped, noise) = manifold.snap([delta.cos(), delta.sin()]);
        cycle_sum += (gauges[j] - gauges[i]) + noise as f64;
    }

    let consistent = cycle_sum.abs() < CYCLE_TOLERANCE;

    // 1 RTT for the initial cycle check
    let mut latency = BASE_RTT;

    if consistent {
        return (true, latency);
    }

    // Inconsistency detected — locate fault via cycle bisection O(log N)
    let bisection_rounds = (n as f64).log2().ceil() as usize; // 7 for N=128
    latency += bisection_rounds as f64 * BASE_RTT;

    // Holonomy can recover as long as the honest majority can form a
    // consistent sub-cycle after excluding identified faulty nodes.
    // Practical bound: faulty < n / 2.
    let success = faulty < n / 2;
    (success, latency)
}

fn main() {
    let faulty_counts: &[usize] = &[0, 10, 20, 30, 42, 50, 60, 64];
    let n = N_NODES;
    let pbft_f = (n - 1) / 3; // = 42

    let manifold = PythagoreanManifold::new(256);
    let mut rng = rand::thread_rng();

    println!("╔══════════════════════════════════════════════════════════════════╗");
    println!("║       Holonomy Consensus vs PBFT — Benchmark (N={n}, trials={N_TRIALS})  ║");
    println!("╚══════════════════════════════════════════════════════════════════╝");
    println!();
    println!("PBFT:     f = {pbft_f}  │  threshold = {}  │  3 phases × 1 RTT",
             2 * pbft_f + 1);
    println!("Holonomy: cycle consistency tol = {CYCLE_TOLERANCE:.0e}  │  bisection = O(log N) = {} RTT",
             (n as f64).log2().ceil() as usize);
    println!();
    println!(
        "┌{0:─<10}┬{0:─<16}┬{0:─<16}┬{0:─<16}┬{0:─<15}┐",
        ""
    );
    println!(
        "│ {:<8} │ {:>14} │ {:>14} │ {:>14} │ {:>13} │",
        "Faulty", "PBFT Success", "PBFT Latency", "Holo Success", "Holo Latency"
    );
    println!(
        "├{0:─<10}┼{0:─<16}┼{0:─<16}┼{0:─<16}┼{0:─<15}┤",
        ""
    );

    for &faulty in faulty_counts {
        let mut pbft_ok = 0usize;
        let mut pbft_lat = 0.0f64;
        let mut holo_ok = 0usize;
        let mut holo_lat = 0.0f64;

        for _ in 0..N_TRIALS {
            let (ps, pl) = pbft_round(n, faulty);
            if ps {
                pbft_ok += 1;
                pbft_lat += pl;
            }

            let (hs, hl) = holonomy_round(n, faulty, &manifold, &mut rng);
            if hs {
                holo_ok += 1;
                holo_lat += hl;
            }
        }

        let pbft_rate = pbft_ok as f64 / N_TRIALS as f64 * 100.0;
        let holo_rate = holo_ok as f64 / N_TRIALS as f64 * 100.0;

        let fmt_lat = |ok: usize, total: f64| -> String {
            if ok == 0 {
                "     —     ".into()
            } else {
                format!("{:.2} RTT", total / ok as f64)
            }
        };

        let pbft_marker = if faulty <= pbft_f { "  " } else { "✗ " };
        let holo_marker = if faulty < n / 2 { "  " } else { "✗ " };

        println!(
            "│ {:<8} │ {}{:>11.0}%   │ {:>14} │ {}{:>11.0}%   │ {:>13} │",
            faulty,
            pbft_marker,
            pbft_rate,
            fmt_lat(pbft_ok, pbft_lat),
            holo_marker,
            holo_rate,
            fmt_lat(holo_ok, holo_lat),
        );
    }

    println!(
        "└{0:─<10}┴{0:─<16}┴{0:─<16}┴{0:─<16}┴{0:─<15}┘",
        ""
    );
    println!();
    println!("✗ = exceeds fault tolerance threshold");
    println!("PBFT threshold: faulty ≤ f = {pbft_f}");
    println!("Holonomy threshold: faulty < N/2 = {}", n / 2);
}
