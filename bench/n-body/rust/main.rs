// n-body (Benchmarks Game), single-threaded Rust baseline with plain f64
// arithmetic (no SIMD), the same algorithm as ../main.cirka.
const PI: f64 = std::f64::consts::PI;
const SOLAR_MASS: f64 = 4.0 * PI * PI;
const DAYS_PER_YEAR: f64 = 365.24;

#[derive(Clone, Copy)]
struct Body {
    x: f64,
    y: f64,
    z: f64,
    vx: f64,
    vy: f64,
    vz: f64,
    m: f64,
}

fn bodies() -> [Body; 5] {
    let d = DAYS_PER_YEAR;
    [
        Body { x: 0.0, y: 0.0, z: 0.0, vx: 0.0, vy: 0.0, vz: 0.0, m: SOLAR_MASS },
        Body {
            x: 4.84143144246472090e+00,
            y: -1.16032004402742839e+00,
            z: -1.03622044471123109e-01,
            vx: 1.66007664274403694e-03 * d,
            vy: 7.69901118419740425e-03 * d,
            vz: -6.90460016972063023e-05 * d,
            m: 9.54791938424326609e-04 * SOLAR_MASS,
        },
        Body {
            x: 8.34336671824457987e+00,
            y: 4.12479856412430479e+00,
            z: -4.03523417114321381e-01,
            vx: -2.76742510726862411e-03 * d,
            vy: 4.99852801234917238e-03 * d,
            vz: 2.30417297573763929e-05 * d,
            m: 2.85885980666130812e-04 * SOLAR_MASS,
        },
        Body {
            x: 1.28943695621391310e+01,
            y: -1.51111514016986312e+01,
            z: -2.23307578892655734e-01,
            vx: 2.96460137564761618e-03 * d,
            vy: 2.37847173959480950e-03 * d,
            vz: -2.96589568540237556e-05 * d,
            m: 4.36624404335156298e-05 * SOLAR_MASS,
        },
        Body {
            x: 1.53796971148509165e+01,
            y: -2.59193146099879641e+01,
            z: 1.79258772950371181e-01,
            vx: 2.68067772490389322e-03 * d,
            vy: 1.62824170038242295e-03 * d,
            vz: -9.51592254519715870e-05 * d,
            m: 5.15138902046611451e-05 * SOLAR_MASS,
        },
    ]
}

fn offset_momentum(b: &mut [Body; 5]) {
    let (mut px, mut py, mut pz) = (0.0, 0.0, 0.0);
    for bi in b.iter() {
        px += bi.vx * bi.m;
        py += bi.vy * bi.m;
        pz += bi.vz * bi.m;
    }
    b[0].vx = -px / SOLAR_MASS;
    b[0].vy = -py / SOLAR_MASS;
    b[0].vz = -pz / SOLAR_MASS;
}

fn energy(b: &[Body; 5]) -> f64 {
    let mut e = 0.0;
    for i in 0..5 {
        let bi = &b[i];
        e += 0.5 * bi.m * (bi.vx * bi.vx + bi.vy * bi.vy + bi.vz * bi.vz);
        for bj in &b[i + 1..] {
            let (dx, dy, dz) = (bi.x - bj.x, bi.y - bj.y, bi.z - bj.z);
            e -= bi.m * bj.m / (dx * dx + dy * dy + dz * dz).sqrt();
        }
    }
    e
}

fn advance(b: &mut [Body; 5], dt: f64) {
    for i in 0..5 {
        for j in i + 1..5 {
            let dx = b[i].x - b[j].x;
            let dy = b[i].y - b[j].y;
            let dz = b[i].z - b[j].z;
            let d2 = dx * dx + dy * dy + dz * dz;
            let mag = dt / (d2 * d2.sqrt());
            let mj = b[j].m * mag;
            let mi = b[i].m * mag;
            b[i].vx -= dx * mj;
            b[i].vy -= dy * mj;
            b[i].vz -= dz * mj;
            b[j].vx += dx * mi;
            b[j].vy += dy * mi;
            b[j].vz += dz * mi;
        }
    }
    for bi in b.iter_mut() {
        bi.x += dt * bi.vx;
        bi.y += dt * bi.vy;
        bi.z += dt * bi.vz;
    }
}

fn main() {
    let n: usize = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(1000);
    let mut b = bodies();
    offset_momentum(&mut b);
    println!("{:.9}", energy(&b));
    for _ in 0..n {
        advance(&mut b, 0.01);
    }
    println!("{:.9}", energy(&b));
}
