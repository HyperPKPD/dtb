// Generates tests/fixtures/*.json from the JavaScript engines of the nlmx apps, so that `cargo test` can check the Rust port against them.
//
//   NLMX_APPS=C:\path\to\nlmx\.Apps node tools/gen_fixtures.mjs
//
// The fixtures are committed; run this only after the JavaScript engines change.
import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

const APPS = process.env.NLMX_APPS || path.resolve('..', 'nlmx', '.Apps');
const imp = (p) => import(pathToFileURL(path.join(APPS, p)).href);
const out = path.resolve('tests', 'fixtures');
fs.mkdirSync(out, { recursive: true });
const save = (name, obj) => fs.writeFileSync(path.join(out, name), JSON.stringify(obj, (_k, v) => (typeof v === 'number' && !Number.isFinite(v) ? (Number.isNaN(v) ? '__NaN__' : v > 0 ? '__Inf__' : '__-Inf__') : v)));

const S = await imp('BEDDI/src/kit/stats.js');
{
  const pts = [-8, -3.2, -1.96, -0.5, 0, 0.3, 1, 1.645, 2.5, 6];
  const rng = S.makeRng(12345), rng2 = S.makeRng(99);
  save('stats.json', {
    normCdf: pts.map((x) => [x, S.normCdf(x)]), normQuantile: [1e-9, 0.001, 0.025, 0.3, 0.5, 0.9, 0.975, 0.999999].map((p) => [p, S.normQuantile(p)]),
    tCdf: [[-2.5, 3], [0.7, 8], [2.228, 10], [4, 25], [1.2, 400]].map(([t, d]) => [t, d, S.tCdf(t, d)]),
    tQuantile: [[0.95, 18], [0.975, 10], [0.9, 3.7], [0.995, 55.3], [0.05, 12]].map(([p, d]) => [p, d, S.tQuantile(p, d)]),
    chi2Cdf: [[3.84, 1], [31.4, 20], [0.5, 4.5]].map(([x, d]) => [x, d, S.chi2Cdf(x, d)]), chi2Quantile: [[0.95, 20], [0.05, 38], [0.975, 7.5]].map(([p, d]) => [p, d, S.chi2Quantile(p, d)]),
    fCdf: [[3.3, 5, 10], [0.4, 12, 30.5]].map(([x, a, b]) => [x, a, b, S.fCdf(x, a, b)]), fQuantile: [[0.95, 5, 10], [0.05, 20, 33], [0.9, 3, 77.7]].map(([p, a, b]) => [p, a, b, S.fQuantile(p, a, b)]),
    lgamma: [0.3, 1, 2.5, 7, 50.5].map((x) => [x, S.lgamma(x)]), betaI: [[0.5, 2, 3], [0.1, 0.5, 4], [0.9, 7, 2.5]].map(([x, a, b]) => [x, a, b, S.betaI(x, a, b)]),
    rng: { seed: 12345, u32: Array.from({ length: 8 }, () => rng.next()), normal: Array.from({ length: 8 }, () => rng.normal()), exp: Array.from({ length: 4 }, () => rng.exp(0.3)), gamma: Array.from({ length: 6 }, () => rng.gamma(0.7)).concat(Array.from({ length: 6 }, () => rng.gamma(4.2))), beta: Array.from({ length: 4 }, () => rng.beta(8, 1.3)), chisq: Array.from({ length: 4 }, () => rng.chisq(9)) },
    rng2: { seed: 99, v: Array.from({ length: 5 }, () => rng2.next()) },
    ols: (() => { const X = [[1, 1, 0], [1, 2, 1], [1, 3, 0], [1, 4, 1], [1, 5, 1], [1, 6, 0]], y = [1.1, 2.3, 2.8, 4.4, 5.1, 5.7]; const f = S.ols(X, y); return { X, y, beta: f.beta, se: f.se, rss: f.rss, df: f.df, rank: f.rank }; })(),
    quantile: [[[3, 1, 4, 1, 5, 9, 2, 6], 0.3], [[1, 2, 3, 4], 0.9]].map(([a, p]) => [a, p, S.quantile(a, p)]),
    integrate: S.integrate((x) => Math.exp(-x * x), -6, 6),
  });
}

// ---- trial
const D = await imp('TRIAL/src/engine/design.js');
const R = await imp('TRIAL/src/engine/run.js');
const P = await imp('TRIAL/src/engine/pk.js');
{
  const base = D.defaultDesign();
  const designs = {};
  const cont = D.defaultDesign(); cont.seed = 11; cont.arms.forEach((a) => { a.n = 30; }); cont.durationWeeks = 6;
  designs.continuous = cont;
  const unc = JSON.parse(JSON.stringify(cont)); unc.uncertainty.enabled = true; unc.decision = { ...unc.decision, kind: 'mcid', mcid: 4, alpha: 0.1 }; unc.dropoutHandling = 'all'; unc.dropout.doseSlope = 0.5;
  designs.continuousUncertain = unc;
  const pr = D.presets(); 
  const resp = JSON.parse(JSON.stringify(pr.find((p) => p.id === 'resp').design)); resp.arms.forEach((a) => { a.n = 40; }); resp.durationWeeks = 4; designs.binary = resp;
  const tte = JSON.parse(JSON.stringify(pr.find((p) => p.id === 'tte').design)); tte.arms.forEach((a) => { a.n = 60; }); tte.durationWeeks = 26; designs.tte = tte;
  const iv = JSON.parse(JSON.stringify(cont)); iv.pk.model = 'iv'; iv.decision.kind = 'significance'; iv.adherence.sd = 0; designs.ivSignificance = iv;
  const fx = { designs: {}, validate: {}, scenarios: {}, base };
  for (const [k, d] of Object.entries(designs)) {
    const r = await R.runScenario(d, { nTrials: 150, seed: 777 });
    delete r.seconds; fx.designs[k] = d; fx.scenarios[k] = r; fx.validate[k] = D.validateDesign(d);
  }
  const bad = JSON.parse(JSON.stringify(cont)); bad.arms = [bad.arms[0]]; bad.pk.CL = -1; bad.decision.alpha = 0.7; fx.validate.bad = D.validateDesign(bad); fx.designs.bad = bad;
  const warn = JSON.parse(JSON.stringify(cont)); warn.arms[0].dose = 10; warn.endpoint.benefit = 'higher'; warn.decision.goThreshold = 9; warn.adherence.sd = 0.9; fx.validate.warn = D.validateDesign(warn); fx.designs.warn = warn;
  fx.overrides = { in: { n: 25, doseScale: 0.5, dropout: 0.03, adherence: 0.8, durationWeeks: 8, ec50Scale: 2, effectScale: 0.7, placeboShift: 1, sdScale: 1.1, clScale: 1.2, uncertainty: true, doses: { a3: 120 } }, out: D.applyOverrides(cont, { n: 25, doseScale: 0.5, dropout: 0.03, adherence: 0.8, durationWeeks: 8, ec50Scale: 2, effectScale: 0.7, placeboShift: 1, sdScale: 1.1, clScale: 1.2, uncertainty: true, doses: { a3: 120 } }) };
  const t = await R.tornado(cont, 'a3', { nTrials: 120, seed: 5 });
  fx.tornado = t;
  fx.bands = R.pkBands(cont, { nSub: 40, points: 30 });
  fx.erCurve = R.erCurve(resp, 20);
  fx.pk = { exposure: P.exposure({ dose: 100, F: 0.8, CL: 4.2, V: 45, ka: 1.3, tau: 12, n: 14, taken: Uint8Array.from([1, 1, 0, 1, 1, 1, 0, 0, 1, 1, 1, 1, 1, 0]), iv: false, concFactor: 1000 }), exposureIv: P.exposure({ dose: 50, F: 1, CL: 4.2, V: 45, ka: 1, tau: 24, n: 5, taken: null, iv: true, concFactor: 1000 }), profile: P.profile({ dose: 100, F: 0.8, CL: 4.2, V: 45, ka: 1.3, tau: 12, n: 4, taken: Uint8Array.from([1, 0, 1, 1]), iv: false, concFactor: 1000 }, [0, 1, 5, 12, 13, 30, 47, 60]) };
  save('trial.json', fx);
}
console.log('fixtures written to', out);

// ---- BEDDI: bioequivalence, power, interactions, NCA, impairment
{
  const BE = await imp('BEDDI/src/engine/be.js');
  const PW = await imp('BEDDI/src/engine/power.js');
  const DDI = await imp('BEDDI/src/engine/ddi.js');
  const NCA = await imp('BEDDI/src/engine/nca.js');
  const IM = await imp('BEDDI/src/engine/impair.js');
  const EX = await imp('BEDDI/src/engine/examples.js');
  const fx = { be: [], residuals: null, power: {}, ddi: {}, nca: {}, impair: {} };
  const map = (params) => ({ subject: 'Subject', sequence: 'Sequence', period: 'Period', treatment: 'Treatment', params });
  const study = (id) => EX.EXAMPLES.find((e) => e.id === id).build();
  const cases = [
    ['crossover', ['Cmax', 'AUClast', 'AUCinf'], [{ method: 'abe' }, { method: 'abe', alpha: 0.05, limits: [0.9, 1.11] }]],
    ['crossover', ['Tmax'], [{ method: 'np' }, { method: 'np', npMargin: [-1, 1] }]],
    ['full', ['Cmax'], [{ method: 'abe' }, { method: 'abel' }, { method: 'rsabe' }, { method: 'abel', abel: { k: 0.76, cvSwitch: 0.3, cvCap: 0.5, peLimits: [0.85, 1.2] } }]],
    ['full', ['AUClast'], [{ method: 'ntid' }, { method: 'abe' }]],
    ['full', ['Tmax'], [{ method: 'np' }]],
    ['partial', ['Cmax', 'AUClast'], [{ method: 'rsabe' }, { method: 'abel' }, { method: 'ntid' }, { method: 'abe' }]],
    ['ntid', ['Cmax', 'AUClast'], [{ method: 'ntid' }]],
    ['parallel', ['Cmax', 'AUClast'], [{ method: 'abe' }, { method: 'abe', welch: false }, { method: 'abel' }]],
  ];
  for (const [id, params, optsList] of cases) {
    const t = study(id); const m = map(params);
    const rec = BE.recordsFromTable(t.columns, t.rows, m);
    for (const p of params) for (const opts of optsList) {
      const r = BE.analyzeBE(rec.records, p, opts);
      fx.be.push({ study: id, table: t, map: m, param: p, options: opts, result: r });
    }
  }
  // missing observations and a zero value
  { const t = study('crossover'); t.rows.splice(3, 1); t.rows[10][4] = '0'; t.rows[12][4] = 'NA'; const m = map(['Cmax']); const rec = BE.recordsFromTable(t.columns, t.rows, m); fx.be.push({ study: 'crossover-missing', table: t, map: m, param: 'Cmax', options: { method: 'abe' }, result: BE.analyzeBE(rec.records, 'Cmax', {}), recordWarnings: rec.warnings }); }
  { const t = study('crossover'); const m = map(['Cmax']); const rec = BE.recordsFromTable(t.columns, t.rows, m); fx.residuals = { table: t, map: m, rows: BE.residualTable(rec.records, 'Cmax', {}) }; }

  // power
  const seqs = { '2x2x2': ['TR', 'RT'], '2x2x3': ['TRT', 'RTR'], '2x2x4': ['TRTR', 'RTRT'], '3x2x3': ['TRR', 'RTR', 'RRT'], '4x2x4': ['TRTR', 'RTRT', 'TRRT', 'RTTR'] };
  fx.power.constants = Object.entries(seqs).map(([k, s]) => { const dc = PW.designConstants(s); return { id: k, seqs: s, c: dc.c, nSeq: dc.nSeq, nPer: dc.nPer, df: [12, 24, 40].map((n) => dc.df(n)), dfR: [12, 24, 40].map((n) => dc.dfR(n)), dfT: [12, 24, 40].map((n) => dc.dfT(n)), hasRrep: dc.hasRrep, hasTrep: dc.hasTrep }; });
  fx.power.constants.push({ id: 'parallel', seqs: null, c: PW.designConstants(null).c, df: [12, 24, 40].map((n) => PW.designConstants(null).df(n)) });
  fx.power.abe = [[0.2, 0.95, 20, '2x2x2'], [0.3, 0.95, 40, '2x2x2'], [0.2, 1, 16, '2x2x2'], [0.3, 1, 32, '2x2x2'], [0.35, 0.9, 30, '2x2x4'], [0.4, 1, 24, '3x2x3'], [0.25, 0.95, 60, 'parallel'], [0.2, 0.95, 24, '2x2x2', [0.9, 1.1111]], [0.3, 1, 28, '2x2x3']].map(([cv, theta0, n, id, limits]) => { const a = { cv, theta0, n, sequences: seqs[id] ?? null, ...(limits ? { limits } : {}) }; return { args: a, power: PW.powerABE(a) }; });
  fx.power.size = [[0.2, 0.95, '2x2x2'], [0.3, 0.9, '2x2x4'], [0.45, 1, '3x2x3'], [0.3, 1, 'parallel']].map(([cv, theta0, id]) => { const a = { cv, theta0, sequences: seqs[id] ?? null }; return { args: a, result: PW.sampleSizeABE(a) }; });
  fx.power.sizeTable = { in: { cvs: [0.15, 0.25, 0.4], ratios: [0.9, 0.95, 1], sequences: seqs['2x2x2'] }, out: PW.sampleSizeTable({ cvs: [0.15, 0.25, 0.4], ratios: [0.9, 0.95, 1], sequences: seqs['2x2x2'] }) };
  fx.power.dropout = [[24, 0.15, 2], [30, 0.2, 3]].map(([n, r, k]) => [n, r, k, PW.withDropout(n, r, k)]);
  fx.power.sim = [
    { method: 'abe', cvT: 0.3, cvR: 0.3, theta0: 0.95, n: 24, sequences: seqs['2x2x4'] },
    { method: 'abel', cvT: 0.4, cvR: 0.45, theta0: 1, n: 36, sequences: seqs['2x2x4'] },
    { method: 'abel', cvT: 0.35, cvR: 0.35, theta0: 0.95, n: 30, sequences: seqs['3x2x3'] },
    { method: 'rsabe', cvT: 0.4, cvR: 0.45, theta0: 0.95, n: 30, sequences: seqs['3x2x3'] },
    { method: 'ntid', cvT: 0.1, cvR: 0.1, theta0: 1, n: 28, sequences: seqs['2x2x4'] },
  ].map((a) => ({ args: { ...a, nSim: 400, seed: 3 }, result: PW.powerSim({ ...a, nSim: 400, seed: 3 }) }));
  fx.power.simSize = (() => { const a = { method: 'abel', cvT: 0.4, cvR: 0.4, theta0: 0.95, sequences: seqs['2x2x4'], nSim: 300, seed: 9, minN: 12, maxN: 120 }; return { args: a, result: PW.sampleSizeSim(a) }; })();

  // interactions
  const cin = { doseMg: 100, mw: 450, cmax: 2.1, fup: 0.05, ka: 1.2, faFg: 1 };
  const enzymes = [{ enzyme: 'CYP3A4', ki: 0.4, kinact: 0.05, KI: 1.5, kdeg: 0.019, kdegGut: 0.03, emax: 6, ec50: 0.3, fm: 0.7, gut: true }, { enzyme: 'CYP2D6', ki: 2, fm: 0.2 }];
  fx.ddi.input = { cin, enzymes };
  fx.ddi.presets = ['fda2020', 'ema2012'].map((id) => { const pr = DDI.PRESETS[id]; const c = DDI.concentrations(cin, pr); return { id, preset: pr, conc: c, static: enzymes.map((e) => DDI.staticR(e, c, pr)), mech: DDI.mechanisticAUCR(enzymes, c, { fg: 0.6 }), mechInlet: DDI.mechanisticAUCR(enzymes, c, { fg: 0.6, hepaticConc: 'inlet', gut: false }) }; });
  fx.ddi.classes = [7, 3, 1.5, 1, 0.7, 0.3, 0.1, NaN].map((r) => [r, DDI.classifyAUCR(r)]);
  fx.ddi.steady = { in: { doseMg: 100, mw: 450, cl: 20, v: 150, ka: 1.2, f: 0.6, tau: 12 }, out: DDI.steadyState({ doseMg: 100, mw: 450, cl: 20, v: 150, ka: 1.2, f: 0.6, tau: 12 }) };
  fx.ddi.oral = [[0, 1, 0.6, 1.2, 0.13, 150], [3, 1000, 0.6, 1.2, 0.13, 150], [2, 100, 1, 0.5, 0.5, 30]].map((a) => [...a, DDI.oralConc(...a)]);
  const dyn = { perp: { doseMg: 200, mw: 450, tau: 24, n: 3, start: 0, cl: 20, v: 150, ka: 1.2, f: 0.8, fup: 0.05 }, victim: { doseMg: 10, mw: 400, tau: 24, n: 1, start: 48, cl: 30, v: 100, ka: 1, f: 0.5, fm: 0.8 }, enzyme: { ki: 0.5, kinact: 0.05, KI: 1.5, emax: 5, ec50: 0.4, kdeg: 0.019 }, tEnd: 96 };
  const cmp = DDI.compareDDI(dyn); 
  fx.ddi.dynamic = { in: dyn, out: { aucr: cmp.aucr, cmaxRatio: cmp.cmaxRatio, aucControl: cmp.aucControl, aucInteraction: cmp.aucInteraction, window: cmp.window, n: cmp.control.t.length, victim: cmp.interaction.victim.filter((_, i) => i % 40 === 0), enzyme: cmp.interaction.enzyme.filter((_, i) => i % 40 === 0), control: cmp.control.victim.filter((_, i) => i % 40 === 0), clRatio: cmp.interaction.clRatio.filter((_, i) => i % 40 === 0) } };

  // NCA
  const ct = EX.concExample(); const cmap = { subject: 'Subject', treatment: 'Treatment', time: 'Time', conc: 'Conc', period: 'Period', sequence: 'Sequence' };
  fx.nca.table = { table: ct, map: cmap, out: (() => { const r = NCA.ncaTable(ct.columns, ct.rows, cmap, { dose: 100 }); return { columns: r.columns, rows: r.rows, profiles: r.profiles.map((p) => ({ subject: p.subject, treatment: p.treatment, period: p.period, result: p.result })) }; })() };
  fx.nca.profiles = [
    { t: [0.5, 1, 2, 4, 8, 12, 24], c: [5, 12, 9, 6, 3, 1.5, 0.4], o: { dose: 100 } },
    { t: [1, 2, 4, 8, 12, 24], c: [0, 3, 8, 4, 2, 0.1], o: { lloq: 0.5, dose: 50 } },
    { t: [0, 1, 2], c: [0, 1, 0], o: {} },
    { t: [0, 1, 2, 4, 6], c: [0, 4, 3, 2.9, 2.8], o: { minPoints: 3, zeroAtStart: false } },
  ].map((p) => ({ ...p, out: NCA.ncaProfile(p.t, p.c, p.o) }));

  // impairment
  const persons = [{ age: 54, weight: 82, height: 175, female: false, scr: 1.3, black: false }, { age: 71, weight: 58, height: 160, female: true, scr: 1.9, black: true, weightType: 'adjusted' }, { age: 33, weight: 70, height: 180, female: false, scr: 0.7, weightType: 'ideal' }];
  fx.impair.egfr = persons.map((p) => ({ p, cg: IM.cockcroftGault(p), mdrd: IM.mdrd(p), ckd2009: IM.ckdEpi2009(p), ckd2021: IM.ckdEpi2021(p), bsaD: IM.bsa(p.height, p.weight), bsaM: IM.bsa(p.height, p.weight, 'mosteller'), ibw: IM.idealWeight(p.height, p.female), denorm: IM.denormalise(IM.ckdEpi2021(p), IM.bsa(p.height, p.weight)) }));
  fx.impair.categories = [120, 90, 75, 59.9, 30, 22, 14, NaN].map((v) => [v, IM.renalCategory(v)?.id ?? null]);
  fx.impair.giusti = { one: IM.giustiHayton({ fe: 0.6, crcl: 45 }), table: IM.giustiHaytonTable({ fe: 0.6, crclNormal: 120, dose: 100, tau: 12 }), noDose: IM.giustiHaytonTable({ fe: 0.3 }) };
  fx.impair.childPugh = [{ bilirubin: 1.2, albumin: 4, inr: 1.1, ascites: 0, encephalopathy: 0 }, { bilirubin: 2.5, albumin: 3, inr: 1.9, ascites: 1, encephalopathy: 0 }, { bilirubin: 4, albumin: 2.5, inr: 2.6, ascites: 2, encephalopathy: 2 }].map((i) => ({ in: i, out: IM.childPugh(i) }));
  fx.impair.liver = [[{ fu: 0.1, clint: 80, bp: 0.9, clRenal: 1, fa: 0.9, fg: 0.8 }, { q: 0.7, clint: 0.4, fu: 1.5 }], [{ q: 80, fu: 0.02, clint: 800 }, { q: 1, clint: 1, fu: 1 }]].map(([i, m]) => ({ in: i, mult: m, out: IM.wellStirred(i, m) }));
  { const t = EX.renalExample(); fx.impair.renalTable = t;
    const gi = t.columns.indexOf(t.columns.find((c) => /group|category/i.test(c)) ?? t.columns[1]);
    const ni = t.columns.findIndex((c) => /auc/i.test(c)); const xi = t.columns.findIndex((c) => /crcl|egfr/i.test(c));
    const groups = []; for (const r of t.rows) { let g = groups.find((x) => x.name === r[gi]); if (!g) groups.push((g = { name: r[gi], values: [] })); g.values.push(Number(r[ni])); }
    fx.impair.groups = { cols: [t.columns[gi], t.columns[ni], t.columns[xi]], groups, ref: groups[0].name, pooled: IM.compareGroups(groups, groups[0].name), welch: IM.compareGroups(groups, groups[0].name, { pooled: false, alpha: 0.05 }) };
    fx.impair.groups.advice = fx.impair.groups.pooled.rows.map((r) => IM.doseAdvice(r));
    const x = t.rows.map((r) => Number(r[xi])), y = t.rows.map((r) => Number(r[ni]));
    fx.impair.regress = { x, y, logX: IM.regressCovariate(x, y, { logX: true, ref: 105, points: [90, 60, 30, 15] }), linear: IM.regressCovariate(x, y, { logX: false, ref: 100, points: [60, 20], alpha: 0.05 }), tooFew: IM.regressCovariate([1, 2], [1, 2]) };
  }
  save('beddi.json', fx);
}
console.log('BEDDI fixtures written');

// ---- prep
{
  const T = await imp('PREP/src/engine/table.js');
  const RC = await imp('PREP/src/engine/recipe.js');
  const EXPR = await imp('PREP/src/engine/expr.js');
  const CK = await imp('PREP/src/engine/checks.js');
  const EXm = await imp('PREP/src/engine/examples.js');
  const EXP = await imp('PREP/src/engine/export.js');
  const files = EXm.exampleFiles();
  const messy = [
    'ID,TIME,AMT,DV,EVID,MDV,CMT,WT,SEX,DATE,Visit Date,RATE,ADDL,II',
    '1,0,100,.,1,1,1,70,M,2024-03-04,04/03/2024,0,2,24', '1,1,0,12.5,0,0,2,70,M,2024-03-04,04/03/2024,0,.,.', '1,2,0,,0,0,2,70,M,2024-03-04,04/03/2024,,.,.', '1,2,0,,0,0,2,70,M,2024-03-04,04/03/2024,,.,.',
    '2,0,0,3.1,0,0,2,.,F,2024-03-05,05/03/2024,,.,.', '2,0,50,5,1,0,1,.,F,2024-03-05,05/03/2024,-3,.,.', '2,-1,0,-2,0,0,2,.,F,2024-03-05,05/03/2024,,.,.',
    '1,5,0,7,0,0,2,70,M,2024-03-04,04/03/2024,,.,.', '3,0,0,0,0,0,2,60,F,2024-03-06,06/03/2024,,.,.', '3,3,10,4,1,1,0,60,F,2024-03-06,06/03/2024,5,1.5,.',
  ].join('\n') + '\n';
  const parse = (txt) => T.parseTable(txt).table;
  const srcT = Object.fromEntries(Object.entries(files).map(([k, v]) => [k, parse(v)]));
  const jt = (t) => JSON.parse(JSON.stringify(t, (k, v) => (typeof v === 'number' && !Number.isFinite(v) ? null : v)));
  const runCase = (name, recipe, srcNames, extraFiles = {}) => {
    const srcs = {}; for (const n of srcNames) srcs[n] = n in extraFiles ? parse(extraFiles[n]) : srcT[n];
    const r = RC.runRecipe(RC.readRecipe(JSON.parse(JSON.stringify(recipe))), srcs);
    return { name, recipe, sources: srcNames, extra: extraFiles, expect: { table: r.table ? jt(r.table) : null, failed: r.failed, steps: jt(r.steps.map((s) => { const { table, ...o } = s; return o; })) } };
  };
  const mk = (steps) => ({ format: 'nlmx-prep-recipe', version: 1, name: 't', description: '', sources: [], steps: steps.map(([op, p], i) => ({ id: `s${i}`, op, enabled: true, note: '', p })) });
  const cases = [];
  cases.push(runCase('example', EXm.exampleRecipe(), ['pk.csv', 'dosing.csv', 'demog.csv']));
  const U = ['use', { source: 'm.csv' }];
  const m = { 'm.csv': messy };
  cases.push(runCase('filter-sort-dedupe', mk([U, ['filter', { action: 'remove', expr: 'EVID == 1 and AMT < 20', why: 'tiny dose' }], ['dedupe', { keys: ['ID', 'TIME', 'EVID'], keep: 'last' }], ['sort', { pairs: [['ID', 'desc'], ['TIME', 'asc']] }], ['columns', { mode: 'first', columns: ['TIME', 'ID'] }]]), ['m.csv'], m));
  cases.push(runCase('derive-groups', mk([U, ['derive', { name: 'N', expr: 'count()', by: ['ID'] }], ['derive', { name: 'LAGT', expr: 'lag(TIME)', by: ['ID'] }], ['derive', { name: 'CUM', expr: 'cumsum(AMT)', by: ['ID'] }], ['derive', { name: 'FT', expr: 'TIME - first(TIME)', by: ['ID'] }], ['derive', { name: 'W', expr: 'coalesce(WT, 70) * 1000', where: 'EVID == 0' }], ['derive', { name: 'S', expr: "if(SEX == 'F', 'fem', concat('m', ID))" }], ['derive', { name: 'R', expr: 'round(DV / 3, 2) + sqrt(abs(DV)) % 2 ^ 2' }], ['derive', { name: 'GM', expr: 'gmedian(DV) + gmax(DV) - gmin(DV)', by: ['ID'] }], ['derive', { name: 'L', expr: 'locf(DV)', by: ['ID'] }], ['derive', { name: 'RN', expr: 'rownum()', by: ['ID'] }]]), ['m.csv'], m));
  cases.push(runCase('derive-bad', mk([U, ['derive', { name: 'X', expr: 'FOO + 1' }]]), ['m.csv'], m));
  cases.push(runCase('tonumber-recode-bin-limits', mk([U, ['recode', { col: 'SEX', out: 'SEXN', pairs: [['m', '1'], ['F', '0']], other: 'missing', ignoreCase: true }], ['bin', { col: 'WT', out: 'WTG', breaks: '65, 60', labels: '', right: true }], ['limits', { col: 'DV', min: 0, max: 10, action: 'cap', flag: 'DVF' }], ['tonumber', { columns: ['Visit Date'] }]]), ['m.csv'], m));
  for (const rule of ['M1', 'M3', 'M4', 'M5', 'M6', 'M7']) cases.push(runCase(`blq-${rule}`, mk([['use', { source: 'b.csv' }], ['blq', { dv: 'DV', lloq: 2, rule, id: 'ID', obs: '', flag: 'BLQ' }]]), ['b.csv'], { 'b.csv': 'ID,TIME,DV\n1,1,<1\n1,2,BLQ\n1,3,4\n1,4,1.5\n2,1,5\n2,2,<1\n2,3,0.5\n2,4,\n2,5,x\n' }));
  cases.push(runCase('blq-error', mk([['use', { source: 'b.csv' }], ['blq', { dv: 'DV', lloq: '', rule: 'M3', id: 'ID' }]]), ['b.csv'], { 'b.csv': 'ID,TIME,DV\n1,1,BLQ\n1,2,4\n' }));
  cases.push(runCase('impute-methods', mk([['use', { source: 'i.csv' }], ['impute', { columns: ['A'], id: 'ID', method: 'locf-only', flag: true }], ['impute', { columns: ['B'], id: 'ID', method: 'nocb', flag: false }], ['impute', { columns: ['C'], id: 'ID', method: 'mean', flag: true }], ['impute', { columns: ['D'], id: 'ID', method: 'mode', flag: true }], ['impute', { columns: ['E'], id: 'ID', method: 'constant', value: '9', flag: true }]]), ['i.csv'], { 'i.csv': 'ID,A,B,C,D,E\n1,,2,1,x,\n1,3,,,x,4\n1,,,,y,\n2,,5,4,,\n2,6,,,y,\n3,,,,,\n' }));
  const cov = ['bmi', 'bsa-mosteller', 'bsa-dubois', 'ibw', 'lbm-james', 'egfr-ckdepi2021', 'egfr-ckdepi2009', 'egfr-mdrd', 'egfr-schwartz'];
  cases.push(runCase('covariates', mk([['use', { source: 'c.csv' }], ...cov.map((f) => ['covariates', { formula: f, wt: 'WT', wtUnit: 'lb', ht: 'HT', htUnit: 'in', age: 'AGE', scr: 'SCR', scrUnit: 'umol/L', sex: 'SEX', male: 'M, 1', out: f.replace(/\W/g, '_') }]), ...['actual', 'ideal', 'adjusted', 'ideal-if-over'].map((w) => ['covariates', { formula: 'crcl-cg', wt: 'WT', wtUnit: 'kg', ht: 'HT', htUnit: 'cm', age: 'AGE', scr: 'SCR', scrUnit: 'mg/dL', sex: 'SEX', male: 'M', weightFor: w, out: `CG_${w.replace(/\W/g, '_')}` }])]), ['c.csv'], { 'c.csv': 'ID,AGE,SEX,WT,HT,SCR\n1,54,M,180,70,100\n2,71,F,130,63,150\n3,33,m,200,72,\n4,8,F,60,120,40\n5,60,X,70,170,90\n' }));
  cases.push(runCase('join-errors', mk([U, ['join', { source: 'd.csv', pairs: [['ID', 'K']], type: 'inner', take: [], dups: 'error' }]]), ['m.csv', 'd.csv'], { ...m, 'd.csv': 'K,G\n1,a\n1,b\n2,c\n' }));
  cases.push(runCase('join-first-inner', mk([U, ['join', { source: 'd.csv', pairs: [['ID', 'K']], type: 'inner', take: [], dups: 'last' }]]), ['m.csv', 'd.csv'], { ...m, 'd.csv': 'K,G,WT\n1,a,1\n1,b,2\n2,c,3\n' }));
  cases.push(runCase('time-tad-addl', mk([['use', { source: 't.csv' }], ['time', { id: 'ID', date: 'D', clock: 'C', format: 'dmy', ref: 'AMT > 0', unit: 'd', out: 'TIME' }], ['tad', { id: 'ID', time: 'TIME', dose: 'AMT > 0', out: 'TAD' }], ['addl', { id: 'ID', time: 'TIME', amt: 'AMT', dose: '', tol: 0.001, min: 3 }]]), ['t.csv'], { 't.csv': 'ID,D,C,AMT,DV\n1,01/03/2024,08:00,10,\n1,02/03/2024,08:00,10,\n1,03/03/2024,8:00 AM,10,\n1,04/03/2024,08:00,10,\n1,04/03/2024,12:30,,5\n2,01/03/2024,0900,20,\n2,03/03/2024,09:00,20,\n2,05/03/2024,09:00,20,\n2,06/03/2024,09:00,,2\n' }));
  cases.push(runCase('time-ambiguous', mk([['use', { source: 't.csv' }], ['time', { id: 'ID', date: 'D', format: 'auto', out: 'TIME' }]]), ['t.csv'], { 't.csv': 'ID,D\n1,01/03/2024\n1,02/03/2024\n' }));
  cases.push(runCase('time-iso-excel', mk([['use', { source: 't.csv' }], ['time', { id: 'ID', date: 'D', format: 'auto', unit: 'min', out: 'TIME' }], ['time', { id: 'ID', date: 'E', format: 'excel', unit: 'h', out: 'T2' }]]), ['t.csv'], { 't.csv': 'ID,D,E\n1,2024-03-04 08:15,45000.5\n1,2024-03-04T09:45:30Z,45001\n1,05-Mar-2024 10:00,45002.25\n2,2024-03-06,45003\n' }));
  cases.push(runCase('nmcols-stack', mk([['use', { source: 'a.csv' }], ['stack', { source: 'b.csv', pairs: [['DV', 'C']], constants: [['EVID', '1'], ['NOTE', 'dose']] }], ['nmcols', { amt: 'AMT', dv: 'DV', evid: '', cmtDose: 1, cmtObs: 2, cmt: '', tidy: true }], ['idmap', { id: 'ID', out: 'NID', keep: 'ORIG', order: 'appearance', start: 100 }]]), ['a.csv', 'b.csv'], { 'a.csv': 'ID,AMT,DV\nB2,0,3\nA10,0,4\nA9,,\n', 'b.csv': 'ID,AMT,C\nB2,50,\nA9,25,\n' }));
  cases.push(runCase('errors', mk([['filter', { expr: 'x' }]]), [], {}));
  const exprTable = parse(messy);
  const exprs = ['WT / (TIME + 1)', "SEX == 'M' and TIME > 0", 'not isna(DV)', 'TIME ^ 2 ^ 0.5', '-TIME ^ 2', "'a' + 'b'", "'a' + 1", '1 + "2"', 'DV > 5 || DV < 0', 'if(DV, 1, 0)', 'ifelse(isna(WT), 1, 2)', 'min(DV, TIME, 3)', 'max()', 'round(2.675, 2)', 'round(-1.5)', 'round(DV)', 'substr(SEX, 1, 1)', "substr('abcdef', 2)", "replace('a-b-c', '-', '')", "replace('abc', '', '-')", "contains(SEX, 'm')", 'len(SEX)', 'upper(SEX)', 'in(ID, 1, 3)', 'between(TIME, 0, 2)', '10 % 3', '-10 % 3', '1 / 0', '[Visit Date]', '`Visit Date`', "DATE", 'time', 'lag(TIME, 2)', 'lead(TIME)', 'cummax(DV)', 'cummin(DV)', 'gany(DV > 10)', 'gall(DV >= 0)', 'gcount(EVID == 1)', 'nocb(DV)', 'trunc(-2.7) + ceil(2.1) + floor(-2.1) + sign(-3)', 'log(0)', 'log10(1000)', 'ln(exp(2))', 'pow(2, 10)', 'pi()', 'sin(0) + cos(0) + tan(0)', 'true', 'na', 'num(" 12 ")', "num('x')", 'str(12.50)', 'trim("  a ")', 'concat(ID, "-", TIME)', 'sqrt(-1)', 'DV == .', 'DV != 5', '1 <> 2', 'a = 1', '1e3 + .5', '(1 + 2', 'foo(1)', 'if(1, 2)', '3 +', '', '@', '"abc'];
  const exprRes = exprs.map((src) => { try { const r = EXPR.evaluate(exprTable, src, { by: ['ID'] }); return { src, values: r.values.map((v) => (typeof v === 'number' && !Number.isFinite(v) ? '__nonfinite__' : v)), columns: r.columns }; } catch (e) { return { src, error: e.message }; } });
  const checkInputs = [
    ['messy', messy, { covariates: ['WT', 'SEX'] }], ['messy-lloq', messy, { lloq: 4, covariates: ['WT'], baseline: false }],
  ];
  const finalRun = RC.runRecipe(RC.readRecipe(EXm.exampleRecipe()), srcT);
  const checks = checkInputs.map(([name, txt, opts]) => ({ name, text: txt, opts, expect: jt(CK.checkDataset(parse(txt), opts)) }));
  checks.push({ name: 'example-final', table: jt(finalRun.table), opts: { covariates: ['AGE', 'SEX', 'WT_KG'], lloq: 5 }, expect: jt(CK.checkDataset(finalRun.table, { covariates: ['AGE', 'SEX', 'WT_KG'], lloq: 5 })) });
  checks.push({ name: 'bad-names', text: 'id,time,dv,x y,DROP,amt\n1,0,5,a,1,0\n1,1,6,b,1,0\n', opts: {}, expect: jt(CK.checkDataset(parse('id,time,dv,x y,DROP,amt\n1,0,5,a,1,0\n1,1,6,b,1,0\n'), {})) });
  checks.push({ name: 'blocks', text: 'ID,TIME,DV\n1,0,5\n2,0,6\n1,1,7\n', opts: {}, expect: jt(CK.checkDataset(parse('ID,TIME,DV\n1,0,5\n2,0,6\n1,1,7\n'), {})) });
  checks.push({ name: 'no-id', text: 'TIME,DV\n0,1\n', opts: {}, expect: jt(CK.checkDataset(parse('TIME,DV\n0,1\n'), {})) });
  const profiles = { m: jt(T.profile(exprTable)), pk: jt(T.profile(srcT['pk.csv'])), demog: jt(T.profile(srcT['demog.csv'])) };
  const exportT = { dataset: EXP.datasetText(finalRun.table, { delimiter: 'comma', missing: '.', header: true }), tab: EXP.datasetText(finalRun.table, { delimiter: 'tab', missing: '', header: false }), nonmem: EXP.nonmemInput(finalRun.table, { file: 'ds.csv' }) };
  const fingerprints = Object.fromEntries(Object.entries(srcT).map(([k, t]) => [k, RC.fingerprint(t)]));
  save('prep.json', { files, messy, cases, exprs: exprRes, exprText: messy, checks, profiles, exportT, fingerprints });
}
console.log('PREP fixtures written');
