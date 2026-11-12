// Writes docs/COMMANDS.md from the command registry of a built dtb.
//
//   node tools/gen_commands_doc.mjs [path-to-dtb]
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';

const DTB = process.argv[2] || path.resolve('target', 'release', process.platform === 'win32' ? 'dtb.exe' : 'dtb');
const r = spawnSync(DTB, ['--no-config', 'info'], { encoding: 'utf8', maxBuffer: 1 << 26 });
if (r.status !== 0) throw new Error(r.stderr);
const info = JSON.parse(r.stdout);
const groups = new Map();
for (const c of info.result.commands) {
  const g = c.name.includes('.') ? c.name.split('.')[0] : 'general';
  if (!groups.has(g)) groups.set(g, []);
  groups.get(g).push(c);
}
const titles = { general: 'General', stats: 'Statistics', prep: 'Prep Studio (data preparation)', trial: 'Trial Simulator', be: 'Bioequivalence', power: 'Power and sample size', ddi: 'Drug–drug interactions', nca: 'Non-compartmental analysis', renal: 'Kidney function', hepatic: 'Liver function' };
let md = `# Command reference\n\nGenerated from dtb ${info.dtb.version} (protocol ${info.dtb.protocol}) by \`tools/gen_commands_doc.mjs\`; \`dtb describe COMMAND\` prints the same for one command.\n\nEvery command takes a JSON object of parameters and returns \`{ "dtb": { …provenance… }, "result": … }\`. Give parameters with \`--input FILE|-\`, \`--file KEY=PATH\`, \`--data FILE\` and \`--set KEY=VALUE\` (see the README). A parameter marked *optional* has a default.\n`;
for (const [g, list] of groups) {
  md += `\n## ${titles[g] || g}\n`;
  for (const c of list) {
    md += `\n### \`${c.name}\`\n\n${c.summary}\n`;
    if (c.params.length) {
      md += `\n| Parameter | | Meaning |\n|---|---|---|\n`;
      for (const p of c.params) md += `| \`${p.name}\` | ${p.required ? 'required' : 'optional'} | ${String(p.description || '').replace(/\|/g, '\\|')} |\n`;
    }
  }
}
fs.mkdirSync('docs', { recursive: true });
fs.writeFileSync(path.join('docs', 'COMMANDS.md'), md);
console.log(`docs/COMMANDS.md: ${info.result.commands.length} commands`);
