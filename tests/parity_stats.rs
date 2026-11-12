mod common;
use common::*;
use dtb::stats::*;

#[test]
fn distributions_match_the_js_library() {
    let fx = fixture("stats.json");
    let t = 1e-10;
    for r in fx["normCdf"].as_array().unwrap() {
        close(norm_cdf(f(&r[0])), f(&r[1]), t, "normCdf");
    }
    for r in fx["normQuantile"].as_array().unwrap() {
        close(norm_quantile(f(&r[0])), f(&r[1]), t, "normQuantile");
    }
    for r in fx["tCdf"].as_array().unwrap() {
        close(t_cdf(f(&r[0]), f(&r[1])), f(&r[2]), t, "tCdf");
    }
    for r in fx["tQuantile"].as_array().unwrap() {
        close(t_quantile(f(&r[0]), f(&r[1])), f(&r[2]), 1e-9, "tQuantile");
    }
    for r in fx["chi2Cdf"].as_array().unwrap() {
        close(chi2_cdf(f(&r[0]), f(&r[1])), f(&r[2]), t, "chi2Cdf");
    }
    for r in fx["chi2Quantile"].as_array().unwrap() {
        close(chi2_quantile(f(&r[0]), f(&r[1])), f(&r[2]), 1e-9, "chi2Quantile");
    }
    for r in fx["fCdf"].as_array().unwrap() {
        close(f_cdf(f(&r[0]), f(&r[1]), f(&r[2])), f(&r[3]), t, "fCdf");
    }
    for r in fx["fQuantile"].as_array().unwrap() {
        close(f_quantile(f(&r[0]), f(&r[1]), f(&r[2])), f(&r[3]), 1e-9, "fQuantile");
    }
    for r in fx["lgamma"].as_array().unwrap() {
        close(lgamma(f(&r[0])), f(&r[1]), t, "lgamma");
    }
    for r in fx["betaI"].as_array().unwrap() {
        close(beta_i(f(&r[0]), f(&r[1]), f(&r[2])), f(&r[3]), t, "betaI");
    }
    for r in fx["quantile"].as_array().unwrap() {
        let a: Vec<f64> = r[0].as_array().unwrap().iter().map(f).collect();
        close(quantile(&a, f(&r[1])), f(&r[2]), t, "quantile");
    }
    close(integrate(|x| (-x * x).exp(), -6.0, 6.0, 8, 32), f(&fx["integrate"]), 1e-12, "integrate");
}

#[test]
fn random_streams_are_identical_to_the_js_generator() {
    let fx = fixture("stats.json");
    let mut r = Rng::new(12345);
    let g = &fx["rng"];
    for v in g["u32"].as_array().unwrap() {
        close(r.next(), f(v), 0.0, "next");
    }
    for v in g["normal"].as_array().unwrap() {
        close(r.std_normal(), f(v), 1e-12, "normal");
    }
    for v in g["exp"].as_array().unwrap() {
        close(r.exp(0.3), f(v), 1e-12, "exp");
    }
    for (i, v) in g["gamma"].as_array().unwrap().iter().enumerate() {
        close(r.gamma(if i < 6 { 0.7 } else { 4.2 }), f(v), 1e-10, "gamma");
    }
    for v in g["beta"].as_array().unwrap() {
        close(r.beta(8.0, 1.3), f(v), 1e-10, "beta");
    }
    for v in g["chisq"].as_array().unwrap() {
        close(r.chisq(9.0), f(v), 1e-10, "chisq");
    }
    let mut r2 = Rng::new(99);
    for v in fx["rng2"]["v"].as_array().unwrap() {
        close(r2.next(), f(v), 0.0, "next2");
    }
}

#[test]
fn least_squares_matches() {
    let fx = fixture("stats.json");
    let o = &fx["ols"];
    let x: Vec<Vec<f64>> = o["X"].as_array().unwrap().iter().map(|r| r.as_array().unwrap().iter().map(f).collect()).collect();
    let y: Vec<f64> = o["y"].as_array().unwrap().iter().map(f).collect();
    let m = ols(&x, &y);
    for (i, b) in o["beta"].as_array().unwrap().iter().enumerate() {
        close(m.beta[i], f(b), 1e-10, "beta");
    }
    for (i, b) in o["se"].as_array().unwrap().iter().enumerate() {
        close(m.se[i], f(b), 1e-10, "se");
    }
    close(m.rss, f(&o["rss"]), 1e-10, "rss");
    assert_eq!(m.df, f(&o["df"]));
    assert_eq!(m.rank as f64, f(&o["rank"]));
}
