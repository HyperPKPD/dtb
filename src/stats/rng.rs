//! A seeded generator: xoshiro128** seeded with splitmix32. The same seed gives the same stream on every machine and in the
//! JavaScript apps.

#[derive(Clone, Debug)]
pub struct Rng {
    a: u32,
    b: u32,
    c: u32,
    d: u32,
    spare: Option<f64>,
}

fn splitmix(s: &mut u32) -> u32 {
    *s = s.wrapping_add(0x9e37_79b9);
    let mut z = *s;
    z = (z ^ (z >> 16)).wrapping_mul(0x85eb_ca6b);
    z = (z ^ (z >> 13)).wrapping_mul(0xc2b2_ae35);
    z ^ (z >> 16)
}

impl Rng {
    /// A generator from a 32-bit seed (higher bits of a larger number are dropped, as in the apps).
    pub fn new(seed: u64) -> Self {
        let mut s = seed as u32;
        let a = splitmix(&mut s);
        let b = splitmix(&mut s);
        let c = splitmix(&mut s);
        let d = splitmix(&mut s);
        Rng { a, b, c, d, spare: None }
    }

    /// An independent stream for chunk `index` of a run with `seed`; the same for any number of threads.
    pub fn for_chunk(seed: u64, index: u64) -> Self {
        // mix seed and index through splitmix64 so that neighbouring chunks are unrelated
        let mut z = seed ^ index.wrapping_mul(0x9e37_79b9_7f4a_7c15).wrapping_add(0x6a09_e667_f3bc_c909);
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^= z >> 31;
        Rng::new(z & 0xffff_ffff)
    }

    #[inline]
    pub fn u32(&mut self) -> u32 {
        let r = self.b.wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = self.b << 9;
        self.c ^= self.a;
        self.d ^= self.b;
        self.b ^= self.c;
        self.a ^= self.d;
        self.c ^= t;
        self.d = self.d.rotate_left(11);
        r
    }

    /// Uniform in [0, 1) with 32 bits.
    #[inline]
    pub fn next(&mut self) -> f64 {
        self.u32() as f64 / 4_294_967_296.0
    }

    /// Uniform in (0, 1), never exactly 0.
    #[inline]
    pub fn open(&mut self) -> f64 {
        (self.u32() as f64 + 0.5) / 4_294_967_296.0
    }

    pub fn uniform(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * (self.u32() as f64 / 4_294_967_296.0)
    }

    pub fn std_normal(&mut self) -> f64 {
        self.normal(0.0, 1.0)
    }

    pub fn normal(&mut self, mean: f64, sd: f64) -> f64 {
        if let Some(v) = self.spare.take() {
            return mean + sd * v;
        }
        let u = self.open();
        let v = self.open();
        let r = (-2.0 * u.ln()).sqrt();
        let ang = 2.0 * std::f64::consts::PI * v;
        self.spare = Some(r * ang.sin());
        mean + sd * r * ang.cos()
    }

    pub fn lognormal(&mut self, meanlog: f64, sdlog: f64) -> f64 {
        self.normal(meanlog, sdlog).exp()
    }

    pub fn exp(&mut self, rate: f64) -> f64 {
        -self.open().ln() / rate
    }

    pub fn bernoulli(&mut self, p: f64) -> bool {
        self.next() < p
    }

    /// Gamma(shape, scale 1), Marsaglia-Tsang.
    pub fn gamma(&mut self, shape: f64) -> f64 {
        if shape < 1.0 {
            let g = self.gamma(shape + 1.0);
            return g * self.open().powf(1.0 / shape);
        }
        let d1 = shape - 1.0 / 3.0;
        let c1 = 1.0 / (9.0 * d1).sqrt();
        loop {
            let mut x;
            let mut v;
            loop {
                x = self.std_normal();
                v = 1.0 + c1 * x;
                if v > 0.0 {
                    break;
                }
            }
            v = v * v * v;
            let u = self.open();
            if u < 1.0 - 0.0331 * x * x * x * x || u.ln() < 0.5 * x * x + d1 * (1.0 - v + v.ln()) {
                return d1 * v;
            }
        }
    }

    pub fn chisq(&mut self, df: f64) -> f64 {
        2.0 * self.gamma(df / 2.0)
    }

    pub fn beta(&mut self, a: f64, b: f64) -> f64 {
        let x = self.gamma(a);
        let y = self.gamma(b);
        x / (x + y)
    }

    /// Integer in [0, n).
    pub fn int(&mut self, n: usize) -> usize {
        (self.next() * n as f64).floor() as usize
    }
}
