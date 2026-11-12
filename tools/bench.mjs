// Times dtb against the apps' JavaScript engines on the same work.
//
//   NLMX_APPS=C:\path\to\nlmx\.Apps node tools/bench.mjs [path-to-dtb]
//
// The numbers differ by machine; what matters is the ratio.
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { spawnSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';

const APPS = process.env.NLMX_APPS || path.resolve('..', 'nlmx', '.Apps');
const DTB = process.argv[2] || path.resolve('target', 'release', process.platform === 'win32' ? 'dtb.exe' : 'dtb');
const imp = (p) => import(pathToFileURL(path.join(APPS, p)).href);
const R = await imp('TRIAL/src/engine/run.js');
const Pw = await imp('BEDDI/src/engine/power.js');
const design = JSON.parse(fs.readFileSync(path.resolve('examples', 'data', 'trial-design.json'), 'utf8'));

const time = async (f) => { const t = process.hrtime.bigint(); const r = await f(); return [Number(process.hrtime.bigint() - t) / 1e6, r]; };
const dtb = (args, threads) => {
  const t = process.hrtime.bigint();
  const r = spawnSync(DTB, ['--threads', String(threads), ...args, '--result'], { encoding: 'utf8', maxBuffer: 1 << 30 });
  if (r.status !== 0) throw new Error(r.stderr || r.stdout);
  return Number(process.hrtime.bigint() - t) / 1e6;
};
const rows = [];
const add = (name, js, d1, dn) => rows.push({ workload: name, 'JS ms': Math.round(js), 'dtb 1 thread ms': Math.round(d1), [`dtb ${os.cpus().length} threads ms`]: Math.round(dn), speedup: (js / dn).toFixed(1) + 'x' });
const tmp = path.join(os.tmpdir(), 'dtb-bench-design.json');
fs.writeFileSync(tmp, JSON.stringify(design));

for (const n of [2000, 10000]) {
  const [js] = await time(() => R.runScenario({ ...design }, { nTrials: n, seed: 7 }));
  const a = ['trial', 'run', '--file', `design=${tmp}`, '--set', `trials=${n}`, '--set', 'seed=7', '--set', 'values=false'];
  add(`trial.run ${n} trials`, js, dtb(a, 1), dtb(a, os.cpus().length));
}
{
  const args = { method: 'rsabe', cvT: 0.4, cvR: 0.4, theta0: 0.95, n: 36, sequences: ['TRR', 'RTR', 'RRT'], nSim: 20000, seed: 3 };
  const [js] = await time(() => Pw.powerSim(args));
  const a = ['power', 'sim', '--set', `args=${JSON.stringify(args)}`];
  add('power.sim RSABE 20000 studies', js, dtb(a, 1), dtb(a, os.cpus().length));
}
console.table(rows);
