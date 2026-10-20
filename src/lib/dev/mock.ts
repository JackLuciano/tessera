/**
 * Dev-only fake backend so the real UI can run in a plain browser (vite dev) for screenshots and
 * layout work. Never bundled into the app: it is imported dynamically behind `import.meta.env.DEV`
 * and only when the Tauri runtime is absent. All names and sizes are made up.
 */
import { mockIPC } from '@tauri-apps/api/mocks';
import { emit } from '@tauri-apps/api/event';
import type { Cat, Kind, TreeNode } from '../api';

interface N { id: number; name: string; parent: number; size: number; files: number; dirs: number; kind: Kind; cat: Cat; modified: number; children: number[] }

const GB = 1024 ** 3, MB = 1024 ** 2;
const now = Math.floor(Date.now() / 1000);
const nodes: N[] = [];
let seed = 7;
const rnd = () => ((seed = (seed * 16807) % 2147483647) / 2147483647);

type Spec = [name: string, size: number, cat: Cat, kids?: Spec[]];
const many = (prefix: string, n: number, total: number, cat: Cat, ext = ''): Spec[] =>
  Array.from({ length: n }, (_, i) => [`${prefix} ${String(i + 1).padStart(2, '0')}${ext}`, total * (0.5 + rnd()) / n, cat] as Spec);

const TREE: Spec = ['C:\\', 0, 'system', [
  ['Users', 0, 'docs', [['alex', 0, 'docs', [
    ['Videos', 0, 'video', [['Vacation 2025', 0, 'video', many('Clip', 14, 48 * GB, 'video', '.mp4')], ['Recordings', 0, 'video', many('Gameplay', 9, 31 * GB, 'video', '.mkv')]]],
    ['Pictures', 0, 'images', [['Camera Roll', 0, 'images', many('IMG', 30, 22 * GB, 'images', '.heic')], ['Wallpapers', 6 * GB, 'images']]],
    ['Documents', 0, 'docs', [['Projects', 0, 'dev', [
      ['webshop', 0, 'dev', [['node_modules', 2.4 * GB, 'dev'], ['.next', 1.1 * GB, 'cache'], ['src', 0.3 * GB, 'dev']]],
      ['game-engine', 0, 'dev', [['target', 9.5 * GB, 'dev'], ['assets', 3.2 * GB, 'images']]],
      ['ml-notebooks', 0, 'dev', [['__pycache__', 0.4 * GB, 'cache'], ['datasets', 12 * GB, 'archives']]],
    ]], ['Invoices', 1.2 * GB, 'docs'], ['Scans', 2.6 * GB, 'docs']]],
    ['Downloads', 0, 'archives', [...many('installer', 6, 8 * GB, 'apps', '.exe'), ...many('backup', 4, 26 * GB, 'archives', '.zip'), ['ubuntu-24.04.iso', 5.8 * GB, 'archives']]],
    ['AppData', 0, 'cache', [['Local', 0, 'cache', [['Temp', 7.2 * GB, 'cache'], ['Google', 0, 'cache', [['Chrome', 0, 'cache', [['Cache', 3.4 * GB, 'cache']]]]], ['npm-cache', 4.1 * GB, 'cache'], ['Packages', 5.5 * GB, 'apps']]], ['Roaming', 6.3 * GB, 'apps']]],
  ]]]],
  ['Games', 0, 'games', [['Steam', 0, 'games', [['steamapps', 0, 'games', [['Starfield', 128 * GB, 'games'], ['Cyberpunk 2077', 71 * GB, 'games'], ['Counter-Strike 2', 34 * GB, 'games'], ['Hades II', 9 * GB, 'games']]]]], ['Epic Games', 0, 'games', [['Fortnite', 46 * GB, 'games']]]]],
  ['Program Files', 0, 'apps', [['Adobe', 14 * GB, 'apps'], ['Microsoft Office', 4.2 * GB, 'apps'], ['NVIDIA Corporation', 2.8 * GB, 'apps'], ['Blender Foundation', 1.9 * GB, 'apps'], ['Docker', 3.6 * GB, 'apps']]],
  ['Program Files (x86)', 0, 'apps', [['Steam', 1.2 * GB, 'apps'], ['Microsoft', 1.6 * GB, 'apps']]],
  ['Windows', 0, 'system', [['WinSxS', 11 * GB, 'system'], ['System32', 7.5 * GB, 'system'], ['SoftwareDistribution', 0, 'system', [['Download', 3.1 * GB, 'system']]], ['Installer', 4.4 * GB, 'system']]],
  ['ProgramData', 0, 'apps', [['Docker', 18 * GB, 'apps'], ['Package Cache', 2.2 * GB, 'cache']]],
  ['pagefile.sys', 16 * GB, 'system'],
  ['hiberfil.sys', 12.7 * GB, 'system'],
]];

function add(s: Spec, parent: number): number {
  const id = nodes.length;
  const [name, size, cat, kids] = s;
  const isDir = !!kids || !/\.\w{2,4}$/.test(name) && size === 0;
  nodes.push({ id, name, parent, size: 0, files: 0, dirs: 0, kind: kids ? 'dir' : /\.\w{2,4}$/.test(name) ? 'file' : 'dir', cat, modified: now - Math.floor(rnd() * 900) * 86400, children: [] });
  if (kids) {
    for (const k of kids) nodes[id].children.push(add(k, id));
  } else if (!isDir && nodes[id].kind === 'dir') {
    // Leaf folder: give it a few big files plus a small-files bucket so drilling down has content
    const n = 3 + Math.floor(rnd() * 4);
    for (let i = 0; i < n; i++) nodes[id].children.push(add([`part-${i + 1}.bin`, size * 0.7 / n, cat], id));
    const b = nodes.length;
    nodes.push({ id: b, name: `${120 + Math.floor(rnd() * 900)} small files`, parent: id, size: size * 0.3, files: 900, dirs: 0, kind: 'bucket', cat, modified: now - 86400 * 3, children: [] });
    nodes[id].children.push(b);
  } else {
    nodes[id].size = size; nodes[id].files = 1;
  }
  const n = nodes[id];
  if (n.children.length) {
    n.size = n.files = n.dirs = 0;
    for (const c of n.children) { const k = nodes[c]; n.size += k.size; n.files += k.files; n.dirs += k.dirs + (k.kind === 'dir' ? 1 : 0); }
    n.children.sort((a, b) => nodes[b].size - nodes[a].size);
  }
  return id;
}
add(TREE, -1);

const pathOf = (id: number): string => {
  const parts: string[] = [];
  for (let c = id; c > 0; c = nodes[c].parent) parts.unshift(nodes[c].name);
  return 'C:\\' + parts.join('\\');
};
const catsOf = (id: number) => {
  const m = new Map<Cat, number>();
  const walk = (i: number) => { const n = nodes[i]; if (!n.children.length) m.set(n.cat, (m.get(n.cat) ?? 0) + n.size); n.children.forEach(walk); };
  walk(id);
  return [...m].map(([cat, size]) => ({ cat, size })).sort((a, b) => b.size - a.size);
};
const dto = (id: number, depth: number, max: number, min: number, top = true): TreeNode => {
  const n = nodes[id];
  return {
    id, name: n.name, path: pathOf(id), size: n.size, files: n.files, dirs: n.dirs, kind: n.kind, cat: catsOf(id)[0]?.cat ?? n.cat,
    modified: n.modified, child_count: n.children.length,
    children: depth > 0 ? n.children.filter(c => nodes[c].size >= (top ? 1 : min)).slice(0, max).map(c => dto(c, depth - 1, max, min, false)) : [],
  };
};
const summary = () => ({ engine: 'mft', root: 'C:\\', size: nodes[0].size, files: nodes[0].files, dirs: nodes[0].dirs, elapsed_ms: 4210, errors: 37, cats: catsOf(0) });
const files = () => nodes.filter(n => n.kind === 'file').sort((a, b) => b.size - a.size);
const junkRule = (key: string, label: string, hint: string, safe: boolean, names: string[]) => {
  const hits = nodes.filter(n => names.includes(n.name));
  return { key, label, hint, safe, size: hits.reduce((s, n) => s + n.size, 0), ids: hits.map(n => n.id), sample: hits.slice(0, 4).map(n => pathOf(n.id)) };
};

let activeProfile = 'gaming';

const sysJunk = () => [
  { key: 'user_temp', safe: true, admin: false, size: 3.3 * GB, count: 929, sample: ['C:\\Users\\alex\\AppData\\Local\\Temp\\setup_2817.tmp'] },
  { key: 'shaders', safe: false, admin: false, size: 3.9 * GB, count: 321, sample: ['C:\\Users\\alex\\AppData\\Local\\NVIDIA\\DXCache'] },
  { key: 'thumbs', safe: true, admin: false, size: 161 * MB, count: 15, sample: [] },
  { key: 'crash', safe: true, admin: false, size: 121 * MB, count: 514, sample: ['C:\\Users\\alex\\AppData\\Local\\CrashDumps\\game.exe.4120.dmp'] },
  { key: 'win_temp', safe: true, admin: true, size: 88 * MB, count: 61, sample: [] },
  { key: 'recycle', safe: false, admin: false, size: 2.1 * GB, count: 43, sample: [] },
];
const privacy = () => [
  { id: 'chrome:cache', group: 'chrome', kind: 'cache', size: 668 * MB, count: 7, running: true },
  { id: 'chrome:cookies', group: 'chrome', kind: 'cookies', size: 2 * MB, count: 2, running: true },
  { id: 'chrome:history', group: 'chrome', kind: 'history', size: 41 * MB, count: 8, running: true },
  { id: 'chrome:session', group: 'chrome', kind: 'session', size: 1 * MB, count: 1, running: true },
  { id: 'firefox:cache', group: 'firefox', kind: 'cache', size: 312 * MB, count: 3, running: false },
  { id: 'firefox:cookies', group: 'firefox', kind: 'cookies', size: 1 * MB, count: 3, running: false },
  { id: 'firefox:history', group: 'firefox', kind: 'history', size: 0, count: 18240, running: false },
  { id: 'system:recent', group: 'system', kind: 'recent', size: 16 * MB, count: 716, running: false },
  { id: 'system:run_history', group: 'system', kind: 'run_history', size: 0, count: 5, running: false },
  { id: 'system:typed_paths', group: 'system', kind: 'typed_paths', size: 0, count: 26, running: false },
  { id: 'system:clipboard', group: 'system', kind: 'clipboard', size: 0, count: 0, running: false },
];
const registry = () => [
  { id: 'HKLM\\U\\a', cat: 'uninstall', name: 'Old Game Launcher 2.1', missing: 'C:\\Games\\OldLauncher\\uninstall.exe', admin: true },
  { id: 'HKCU\\U\\b', cat: 'uninstall', name: 'Photo Tool', missing: 'C:\\Users\\alex\\AppData\\Local\\PhotoTool\\unins000.exe', admin: false },
  { id: 'HKCU\\R|c', cat: 'startup', name: 'Updater', missing: 'C:\\Program Files\\Vendor\\updater.exe', admin: false },
  { id: 'HKLM\\A\\d', cat: 'app_paths', name: 'oldtool.exe', missing: 'C:\\Tools\\oldtool.exe', admin: true },
  ...Array.from({ length: 9 }, (_, i) => ({ id: `HKCU\\M|${i}`, cat: 'mui_cache', name: `C:\\Temp\\setup${i}.exe.FriendlyAppName`, missing: `C:\\Temp\\setup${i}.exe`, admin: false })),
];
const shortcuts = () => [
  { path: 'C:\\Users\\alex\\Desktop\\Old Game.lnk', name: 'Old Game', target: 'D:\\Games\\OldGame\\game.exe', location: 'desktop', admin: false },
  { path: 'C:\\Users\\alex\\AppData\\Roaming\\Microsoft\\Windows\\Start Menu\\Programs\\Photo Tool.lnk', name: 'Photo Tool', target: 'C:\\Users\\alex\\AppData\\Local\\PhotoTool\\phototool.exe', location: 'start_menu', admin: false },
  { path: 'C:\\ProgramData\\Microsoft\\Windows\\Start Menu\\Programs\\Vendor\\Uninstall.lnk', name: 'Uninstall Vendor App', target: 'C:\\Program Files\\Vendor\\unins000.exe', location: 'start_menu', admin: true },
];
const programs = () => [
  ['Blender', 'Blender Foundation', '4.5.2', 912], ['Discord', 'Discord Inc.', '1.0.9201', 410], ['Docker Desktop', 'Docker Inc.', '4.92.0', 2400],
  ['GIMP 3.0.4', 'The GIMP Team', '3.0.4', 380], ['Git', 'The Git Development Community', '2.54.0', 340], ['OBS Studio', 'OBS Project', '31.1', 520],
  ['Steam', 'Valve Corporation', '2.10.91.91', 0], ['Visual Studio Code', 'Microsoft Corporation', '1.141.1', 420], ['VLC media player', 'VideoLAN', '3.0.21', 180],
].map(([name, publisher, version, mb], i) => ({ id: `HKLM\\U\\${i}`, name, publisher, version, installed: now - (i * 37 + 9) * 86400, size: (mb as number) * MB, location: '', machine: i % 2 === 0 }));
const updates = () => [
  { name: 'Docker Desktop', id: 'Docker.DockerDesktop', version: '4.92.0', available: '4.95.0', source: 'winget' },
  { name: 'Git', id: 'Git.Git', version: '2.54.0', available: '2.55.0.5', source: 'winget' },
  { name: 'Microsoft Visual Studio Code', id: 'Microsoft.VisualStudioCode', version: '1.141.1', available: '1.142.0', source: 'winget' },
  { name: 'Python 3.14.5', id: 'Python.Python.3.14', version: '3.14.5', available: '3.14.7', source: 'winget' },
  { name: 'Telegram Desktop', id: 'Telegram.TelegramDesktop', version: '7.1.2', available: '7.3.1', source: 'winget' },
];
const mockGames = [
  { id: 'steam:730', name: 'Counter-Strike 2', launcher: 'steam', dir: 'C:\\Games\\Steam\\steamapps\\common\\Counter-Strike Global Offensive', size: 34 * GB, last_played: now - 3600, playtime: 41_200, cover: 'https://cdn.cloudflare.steamstatic.com/steam/apps/730/library_600x900.jpg', launch: 'steam://rungameid/730', shader_cache: ['C:\\Games\\Steam\\steamapps\\shadercache\\730', 1.4 * GB] },
  { id: 'steam:1145350', name: 'Hades II', launcher: 'steam', dir: 'C:\\Games\\Steam\\steamapps\\common\\Hades II', size: 9 * GB, last_played: now - 86400 * 2, playtime: 2_300, cover: 'https://cdn.cloudflare.steamstatic.com/steam/apps/1145350/library_600x900.jpg', launch: 'steam://rungameid/1145350', shader_cache: null },
  { id: 'steam:1091500', name: 'Cyberpunk 2077', launcher: 'steam', dir: 'C:\\Games\\Steam\\steamapps\\common\\Cyberpunk 2077', size: 71 * GB, last_played: now - 86400 * 140, playtime: 9_800, cover: 'https://cdn.cloudflare.steamstatic.com/steam/apps/1091500/library_600x900.jpg', launch: 'steam://rungameid/1091500', shader_cache: ['C:\\Games\\Steam\\steamapps\\shadercache\\1091500', 2.2 * GB] },
  { id: 'epic:Fortnite', name: 'Fortnite', launcher: 'epic', dir: 'C:\\Games\\Epic Games\\Fortnite', size: 46 * GB, last_played: 0, playtime: 0, cover: null, launch: 'com.epicgames.launcher://apps/Fortnite?action=launch', shader_cache: null },
  { id: 'riot:VALORANT', name: 'VALORANT', launcher: 'riot', dir: 'C:\\Riot Games\\VALORANT', size: 34 * GB, last_played: 0, playtime: 0, cover: null, launch: null, shader_cache: null },
  { id: 'xbox:Forza Horizon 5', name: 'Forza Horizon 5', launcher: 'xbox', dir: 'C:\\XboxGames\\Forza Horizon 5', size: 128 * GB, last_played: 0, playtime: 0, cover: null, launch: null, shader_cache: null },
];
let gmCfg = {
  enabled: true, default_boost: 'ask', switch_plan: true, apply_profile: false, close: ['onedrive', 'qbittorrent', 'ms-teams'], keep: ['discord', 'spotify'],
  reopen: true, clear_standby: true, timer: true, unpark: false, record_fps: true, overlay: true, custom: [],
  per_game: { 'steam:730': { boost: 'always', close: ['chrome'], keep: [] } } as Record<string, unknown>,
};
let pingSamples: (number | null)[] = Array.from({ length: 40 }, () => 14 + rnd() * 9);
const snapshot = (i = 120) => ({
  at: Date.now() - (120 - i) * 2000, cpu_load: 28 + Math.sin(i / 7) * 14 + rnd() * 8, cpu_mhz: 4620, cpu_base_mhz: 3600, cpu_limit: 100, cpu_temp: 61 + Math.sin(i / 9) * 6, cpu_temp_approx: true,
  gpus: [{ name: 'NVIDIA GeForce RTX 4070', load: 72 + Math.sin(i / 5) * 18, temp: 66 + Math.sin(i / 8) * 4, mhz: 2610, max_mhz: 3105, mem_used: 7.1 * GB, mem_total: 12 * GB, power: 168, power_limit: 200, fan: 54, throttle: [] }],
  ram_used: 18.2 * GB, ram_total: 32 * GB, ram_standby: 9.4 * GB, sensors: [['ACPI\\ThermalZone\\TZ00_0', 61]], fans: [],
});
let careCfg = {
  modules: ['junk', 'privacy', 'registry', 'shortcuts', 'startup', 'disk', 'updates'],
  privacy_kinds: ['cache', 'session', 'recent', 'run_history', 'typed_paths', 'search_history'],
  schedule: 'weekly', time: '12:00', auto_clean: false, last_run: now - 3 * 86400, last_score: 71, last_found: 3.6 * GB, last_issues: 31,
};
const careReport = () => {
  const junk = sysJunk(), priv = privacy(), reg = registry(), links = shortcuts();
  const plan = {
    junk: junk.filter(j => j.safe).map(j => j.key),
    privacy: priv.filter(p => careCfg.privacy_kinds.includes(p.kind) && (!p.running || p.kind === 'cache')).map(p => p.id),
    registry: reg.map(r => r.id), shortcuts: links.filter(l => !l.admin).map(l => l.path),
  };
  return {
    at: Math.floor(Date.now() / 1000), score: 68, junk, privacy: priv, registry: reg, shortcuts: links, startup: 11,
    disks: [{ id: '1', name: 'Samsung SSD 990 PRO', media: 'ssd', bus: 'NVMe', health: 'healthy', size: 2000 * GB, temperature: 41, wear: 3, power_on_hours: 3120, read_errors: 0, volumes: ['C:'] }],
    updates: updates(), fixable_bytes: 4.1 * GB, fixable_issues: 37, plan,
  };
};

/** Command call counts, for checking that views don't refetch too often (`window.__mockCalls`). */
const calls: Record<string, number> = ((window as unknown as { __mockCalls: Record<string, number> }).__mockCalls = {});
let liveTimer: ReturnType<typeof setInterval> | undefined;

async function fakeScan() {
  const total = nodes[0];
  for (let i = 1; i <= 12; i++) {
    await new Promise(r => setTimeout(r, 110));
    await emit('scan-progress', { files: Math.round(total.files * i / 12), dirs: Math.round(total.dirs * i / 12), bytes: Math.round(total.size * i / 12) });
  }
  // Like a busy system drive: a few folders change every second
  clearInterval(liveTimer);
  liveTimer = setInterval(() => {
    total.size += 4096;
    emit('live-change', { folders: 3, delta: 4096, size: total.size, files: total.files });
  }, 1000);
  return summary();
}

export function installMock() {
  mockIPC(async (cmd, args) => {
    const a = (args ?? {}) as Record<string, any>;
    calls[cmd] = (calls[cmd] ?? 0) + 1;
    switch (cmd) {
      case 'list_drives': return [
        { name: 'Windows', mount: 'C:\\', fs: 'NTFS', total: 1000 * GB, available: 1000 * GB - nodes[0].size * 1.04, removable: false, kind: 'SSD' },
        { name: 'Data', mount: 'D:\\', fs: 'NTFS', total: 2000 * GB, available: 1240 * GB, removable: false, kind: 'HDD' },
        { name: 'USB', mount: 'E:\\', fs: 'exFAT', total: 128 * GB, available: 91 * GB, removable: true, kind: 'Unknown' },
      ];
      case 'is_elevated': return true;
      case 'scan': return fakeScan();
      case 'load_cached':
        if (new URLSearchParams(location.search).has('fresh')) return null;
        setTimeout(() => emit('catchup-done', { folders: 214, delta: 1.3 * GB, size: nodes[0].size, files: nodes[0].files }), 2500);
        return { ...summary(), engine: 'cache', cached_at: now - 3 * 3600 };
      case 'get_summary': return summary();
      case 'get_node': return dto(a.id, Math.min(a.depth, 6), a.maxChildren, a.minFrac ? nodes[a.id].size * a.minFrac : 0);
      case 'get_ancestors': { const out: [number, string][] = []; for (let c = a.id; c >= 0; c = nodes[c].parent) out.unshift([c, nodes[c].name]); return out; }
      case 'largest_files': return files().slice(0, a.limit).map(n => ({ id: n.id, name: n.name, path: pathOf(n.id), size: n.size, cat: n.cat, modified: n.modified }));
      case 'find_junk': return [
        junkRule('node_modules', 'node_modules folders', 'Reinstall with npm/pnpm install', true, ['node_modules']),
        junkRule('cargo_target', 'Rust target folders', 'Rebuilt by cargo build', true, ['target']),
        junkRule('temp', 'Temp folders', 'Temporary files left behind by apps', true, ['Temp']),
        junkRule('pkg_cache', 'Package manager caches', 'npm, pnpm, pip, NuGet, yarn — re-downloaded on demand', true, ['npm-cache']),
        junkRule('browser', 'Browser caches', 'Chrome / Edge / Firefox cache, logins are not touched', true, ['Cache']),
        junkRule('next', 'Next.js / Nuxt build caches', '.next, .nuxt, .svelte-kit — rebuilt on next build', true, ['.next']),
        junkRule('pycache', 'Python caches', '__pycache__ and .pytest_cache', true, ['__pycache__']),
        junkRule('wu', 'Windows Update downloads', 'Already-installed update packages', false, ['Download']),
      ].filter(g => g.size > 0).sort((x, y) => y.size - x.size);
      case 'find_duplicates': {
        for (let i = 1; i <= 6; i++) { await new Promise(r => setTimeout(r, 120)); await emit('dup-progress', { hashed: i * 4 * GB, total: 24 * GB }); }
        const f = files().filter(n => n.name.endsWith('.zip') || n.name.endsWith('.mp4')).slice(0, 9);
        return [0, 3, 6].map(i => ({ size: f[i].size, wasted: f[i].size * 2, files: f.slice(i, i + 3).map(n => ({ id: n.id, path: pathOf(n.id), modified: n.modified })) }));
      }
      case 'history_list': return Array.from({ length: 9 }, (_, i) => ({ ts: now - (8 - i) * 6 * 86400, size: nodes[0].size * (0.82 + i * 0.022 + rnd() * 0.01), files: Math.round(nodes[0].files * (0.9 + i * 0.012)) }));
      case 'history_diff': return [
        { path: 'C:\\Users\\alex\\Videos', before: 52 * GB, after: 79 * GB, delta: 27 * GB },
        { path: 'C:\\Games\\Steam\\steamapps', before: 196 * GB, after: 242 * GB, delta: 46 * GB },
        { path: 'C:\\Users\\alex\\Downloads', before: 61 * GB, after: 44 * GB, delta: -17 * GB },
        { path: 'C:\\ProgramData\\Docker', before: 12 * GB, after: 18 * GB, delta: 6 * GB },
        { path: 'C:\\Windows\\SoftwareDistribution', before: 5.2 * GB, after: 3.1 * GB, delta: -2.1 * GB },
      ];
      case 'opt_status': return {
        supported: true, elevated: true, active: activeProfile, power_plans: [
          { guid: '381b4222-f694-41f0-9685-ff5bb260df2e', name: 'Balanced', active: activeProfile !== 'gaming' },
          { guid: '8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c', name: 'High performance', active: activeProfile === 'gaming' },
          { guid: 'e9a42b02-d5df-448d-aa00-03f14749eb61', name: 'Ultimate Performance', active: false },
        ],
        tweaks: [
          { key: 'game_mode', on: activeProfile !== 'work' && activeProfile !== 'battery', admin: false, reboot: false },
          { key: 'no_game_dvr', on: activeProfile === 'gaming', admin: false, reboot: false },
          { key: 'perf_visuals', on: activeProfile === 'gaming' || activeProfile === 'battery', admin: false, reboot: true },
          { key: 'no_transparency', on: activeProfile === 'battery', admin: false, reboot: false },
          { key: 'no_background_apps', on: activeProfile === 'gaming' || activeProfile === 'battery', admin: false, reboot: false },
          { key: 'raw_mouse', on: activeProfile === 'gaming', admin: false, reboot: false },
          { key: 'focus', on: activeProfile === 'gaming' || activeProfile === 'work', admin: false, reboot: false },
          { key: 'hags', on: activeProfile === 'gaming', admin: true, reboot: true },
          { key: 'net_throttling', on: activeProfile === 'gaming', admin: true, reboot: true },
          { key: 'game_priority', on: activeProfile === 'gaming', admin: true, reboot: true },
          { key: 'no_power_throttling', on: activeProfile === 'gaming', admin: true, reboot: true },
          { key: 'no_gamebar', on: activeProfile === 'gaming', admin: false, reboot: false },
          { key: 'no_web_search', on: activeProfile === 'work', admin: false, reboot: false },
          { key: 'no_suggestions', on: activeProfile === 'work', admin: false, reboot: false },
          { key: 'no_mpo', on: true, admin: true, reboot: true },
          { key: 'end_task', on: true, admin: false, reboot: false },
          { key: 'no_activity', on: false, admin: true, reboot: false },
          { key: 'no_update_sharing', on: false, admin: true, reboot: false },
        ],
        has_backup: activeProfile !== 'balanced',
        memory: { total: 32 * GB, used: 17.4 * GB }, cpu: 23,
      };
      case 'sys_stats': return { memory: { total: 32 * GB, used: (16 + rnd() * 3) * GB }, cpu: 12 + rnd() * 30 };
      case 'opt_set_power_plan': return null;
      case 'opt_apply_profile': activeProfile = a.profile; return { applied: 6, failed: [], reboot: a.profile === 'gaming' };
      case 'opt_set_tweak': return { applied: 1, failed: [], reboot: false };
      case 'opt_restore': activeProfile = 'balanced'; return { applied: 7, failed: [], reboot: false };
      case 'opt_startup_list': return [
        { id: 'hkcu:Discord', name: 'Discord', command: 'C:\\Users\\alex\\AppData\\Local\\Discord\\Update.exe --processStart Discord.exe', enabled: true, scope: 'user' },
        { id: 'hkcu:Spotify', name: 'Spotify', command: 'C:\\Users\\alex\\AppData\\Roaming\\Spotify\\Spotify.exe /minimized', enabled: true, scope: 'user' },
        { id: 'hkcu:Steam', name: 'Steam', command: 'C:\\Program Files (x86)\\Steam\\steam.exe -silent', enabled: false, scope: 'user' },
        { id: 'hklm:SecurityHealth', name: 'SecurityHealth', command: '%windir%\\system32\\SecurityHealthSystray.exe', enabled: true, scope: 'machine' },
        { id: 'hklm:RtkAudUService', name: 'RtkAudUService', command: 'C:\\Windows\\System32\\RtkAudUService64.exe -background', enabled: true, scope: 'machine' },
      ];
      case 'opt_startup_set': return null;
      case 'opt_processes': return [
        { pids: [4120, 4121, 4122], name: 'chrome.exe', memory: 3.1 * GB, cpu: 4.2, group: 'browser' },
        { pids: [6620], name: 'ms-teams.exe', memory: 0.9 * GB, cpu: 1.1, group: 'chat' },
        { pids: [9932, 9933], name: 'Discord.exe', memory: 0.6 * GB, cpu: 0.4, group: 'chat' },
        { pids: [5501], name: 'EpicGamesLauncher.exe', memory: 0.35 * GB, cpu: 0.8, group: 'launcher' },
        { pids: [8800], name: 'Spotify.exe', memory: 0.32 * GB, cpu: 0.6, group: 'media' },
        { pids: [7710], name: 'OneDrive.exe', memory: 0.21 * GB, cpu: 0.1, group: 'sync' },
        { pids: [3300], name: 'vmmemWSL', memory: 4.2 * GB, cpu: 0.9, group: 'dev' },
        { pids: [3310], name: 'Docker Desktop.exe', memory: 0.4 * GB, cpu: 0.3, group: 'dev' },
      ];
      case 'opt_kill': return { applied: (a.pids ?? []).length, failed: [], reboot: false };
      case 'set_live_watch': case 'cancel_scan': case 'cancel_duplicates': case 'reveal': return null;
      case 'delete_nodes': {
        const ids = (a.ids as number[]).filter(id => nodes[id]);
        const total_bytes = ids.reduce((s, id) => s + nodes[id].size, 0);
        for (let i = 1; i <= 20; i++) {
          await new Promise(r => setTimeout(r, 150));
          await emit('delete-progress', { done: Math.round(ids.length * i / 20), total: ids.length, bytes: total_bytes * i / 20, total_bytes });
        }
        return { freed: total_bytes, deleted: ids.length, failed: [], cancelled: false };
      }
      case 'cancel_delete': return null;
      case 'hardlink_dupes': return { freed: 12 * GB, deleted: a.pairs.length, failed: [] };
      case 'prefs_get': return { close_to_tray: true, tray_meter: true, tray_hint_shown: true, widget: false, widget_on_top: true, hotkeys_enabled: true, hotkeys: { fps: 'Ctrl+Shift+F', boost: 'Ctrl+Shift+B', care: 'Ctrl+Shift+C', widget: 'Ctrl+Shift+W' }, weekly: true };
      case 'prefs_set': return [];
      case 'tray_labels': return null;
      case 'launch_state': return { hidden: false };
      case 'junk_scan': return sysJunk();
      case 'junk_clean': return { freed: 3.4 * GB, cleaned: 812, locked: 14, failed: [] };
      case 'privacy_scan': return privacy();
      case 'privacy_clean': return { freed: 0.9 * GB, cleaned: (a.ids ?? []).length, failed: [], running: [] };
      case 'registry_scan': return registry();
      case 'registry_fix': return { fixed: (a.ids ?? []).length, failed: [], backup: '1760000000.json', restore_point: 'created' };
      case 'registry_backups': return [{ name: '1760000000.json', created: now - 2 * 86400, items: 12 }];
      case 'registry_undo': return 12;
      case 'shortcuts_scan': return shortcuts();
      case 'shortcuts_fix': return { fixed: (a.paths ?? []).length, failed: [] };
      case 'empty_folders': return [
        { id: nodes.findIndex(n => n.name === 'Wallpapers'), path: 'C:\\Users\\alex\\Documents\\Old project\\assets', nested: 3 },
        { id: nodes.findIndex(n => n.name === 'Invoices'), path: 'C:\\Users\\alex\\Downloads\\New folder', nested: 0 },
        { id: nodes.findIndex(n => n.name === 'Scans'), path: 'C:\\Games\\Mods\\backup', nested: 1 },
      ];
      case 'shred_inspect': return (a.paths as string[]).map(p => ({ path: p, size: 1.2 * GB, files: 14, is_dir: !/\.\w+$/.test(p) }));
      case 'shred_paths': return { freed: 1.2 * GB, deleted: (a.paths ?? []).length, failed: [], cancelled: false };
      case 'disk_health': return {
        disks: [
          { id: '1', name: 'Samsung SSD 990 PRO 2TB', media: 'ssd', bus: 'NVMe', health: 'healthy', size: 2000 * GB, temperature: 41, wear: 3, power_on_hours: 3120, read_errors: 0, volumes: ['C:'] },
          { id: '0', name: 'WDC WD40EZAZ', media: 'hdd', bus: 'SATA', health: 'warning', size: 4000 * GB, temperature: 47, wear: null, power_on_hours: 28411, read_errors: 12, volumes: ['D:'] },
        ], counters_need_admin: false, can_optimize: true,
      };
      case 'disk_optimize': await new Promise(r => setTimeout(r, 1500)); return a.volume;
      case 'programs_list': return programs();
      case 'program_uninstall': await new Promise(r => setTimeout(r, 1200)); return true;
      case 'program_leftovers': return [
        { kind: 'folder', path: `C:\\Users\\alex\\AppData\\Roaming\\${a.program.name}`, size: 180 * MB, admin: false },
        { kind: 'registry', path: `HKCU\\Software\\${a.program.name}`, size: 0, admin: false },
      ];
      case 'leftovers_remove': return { removed: (a.items ?? []).length, freed: 180 * MB, failed: [], backup: null, restore_point: 'session' };
      case 'updates_list': return { available: true, items: updates() };
      case 'update_install': await new Promise(r => setTimeout(r, 900)); return null;
      case 'net_status': return {
        adapters: [{ index: 10, name: 'Ethernet', description: 'Intel(R) Ethernet Connection I219-V', guid: '{0}', servers: ['192.168.0.1'], manual: false, preset: 'auto' }],
        tcp: [{ key: 'autotuning', on: true, value: 'Normal' }, { key: 'rss', on: false, value: 'Disabled' }, { key: 'nagle', on: false, value: '' }],
        has_backup: true, elevated: true,
      };
      case 'net_latency': await new Promise(r => setTimeout(r, 700)); return [
        { key: 'current', server: '192.168.0.1', ms: 17 }, { key: 'cloudflare', server: '1.1.1.1', ms: 11 }, { key: 'google', server: '8.8.8.8', ms: 24 },
        { key: 'quad9', server: '9.9.9.9', ms: 19 }, { key: 'opendns', server: '208.67.222.222', ms: null },
      ];
      case 'net_set_dns': return 1;
      case 'net_flush': case 'net_set_tcp': case 'net_repair': return null;
      case 'net_restore': return 2;
      case 'care_config': return { ...careCfg, running: false, can_schedule: true };
      case 'care_set_config': careCfg = { ...careCfg, ...a.config }; return null;
      case 'care_scan': await new Promise(r => setTimeout(r, 1800)); return careReport();
      case 'care_fix': return { freed: 4.1 * GB, fixed: 37, failed: [], running: [], restore_point: 'created', registry_backup: '1760000000.json' };
      case 'games_list': return mockGames;
      case 'game_launch': case 'game_set_exe': case 'gamemode_answer': case 'gamemode_boost_now': case 'overlay_set': case 'widget_set':
      case 'display_set_refresh': case 'display_set_switch': case 'core_parking_set': case 'open_uri': case 'fps_start': case 'fps_delete':
      case 'winupdate_reset': case 'settings_export': return null;
      case 'game_exes': return [['C:\\Games\\Steam\\steamapps\\common\\Counter-Strike Global Offensive\\game\\bin\\win64\\cs2.exe', { priority: 'high', gpu_high: true, no_fso: false }]];
      case 'game_leftovers': return [{ path: 'D:\\SteamLibrary\\steamapps\\common\\Old Racing Game', size: 14.2 * GB, launcher: 'steam' }];
      case 'game_remove_paths': return { freed: 16 * GB, failed: [] };
      case 'gamemode_config': return gmCfg;
      case 'gamemode_set_config': gmCfg = a.config; return null;
      case 'gamemode_status': return { game: mockGames[0], boosted: true, since: now - 47 * 60, closed: ['onedrive', 'qbittorrent'], pending: null };
      case 'gamemode_sessions': return [
        { game_id: 'steam:730', game: 'Counter-Strike 2', start: now - 86400 - 5400, end: now - 86400, boosted: true, closed: ['onedrive'], throttle: [], fps: { game: 'Counter-Strike 2', exe: 'cs2.exe', start: now - 86400 - 5400, seconds: 5400, frames: 1_500_000, avg: 278, low1: 164, low01: 121, timeline: [] } },
        { game_id: 'epic:Fortnite', game: 'Fortnite', start: now - 3 * 86400 - 7200, end: now - 3 * 86400, boosted: true, closed: [], throttle: [{ start: 0, end: 60000, kind: 'gpu_thermal', worst: 87 }], fps: { game: 'Fortnite', exe: 'FortniteClient-Win64-Shipping.exe', start: 0, seconds: 7200, frames: 1_000_000, avg: 141, low1: 92, low01: 70, timeline: [] } },
      ];
      case 'running_apps': return [
        { id: 'chrome', name: 'chrome', memory: 3.1 * GB }, { id: 'discord', name: 'Discord', memory: 0.6 * GB }, { id: 'onedrive', name: 'OneDrive', memory: 0.2 * GB },
        { id: 'spotify', name: 'Spotify', memory: 0.3 * GB }, { id: 'ms-teams', name: 'ms-teams', memory: 0.9 * GB }, { id: 'steamwebhelper', name: 'steamwebhelper', memory: 0.4 * GB },
      ];
      case 'fps_status': return { tool_ready: true, recording: null, elevated: true, overlay: false };
      case 'fps_stop': return null;
      case 'fps_sessions': return [
        { game: 'Counter-Strike 2', exe: 'cs2.exe', start: now - 86400, seconds: 5400, frames: 1_500_000, avg: 278, low1: 164, low01: 121, timeline: Array.from({ length: 90 }, (_, i) => 260 + Math.sin(i / 5) * 25 + rnd() * 20) },
        { game: 'Counter-Strike 2', exe: 'cs2.exe', start: now - 6 * 86400, seconds: 3600, frames: 900_000, avg: 241, low1: 131, low01: 98, timeline: Array.from({ length: 60 }, (_, i) => 230 + Math.cos(i / 6) * 30 + rnd() * 20) },
        { game: 'Fortnite', exe: 'FortniteClient-Win64-Shipping.exe', start: now - 3 * 86400, seconds: 7200, frames: 1_000_000, avg: 141, low1: 92, low01: 70, timeline: Array.from({ length: 120 }, (_, i) => 140 + Math.sin(i / 8) * 18 + rnd() * 12) },
      ];
      case 'sensors_now': return snapshot();
      case 'sensors_history': return { history: Array.from({ length: 120 }, (_, i) => snapshot(i)), throttle: [{ start: Date.now() - 3 * 3600_000, end: Date.now() - 3 * 3600_000 + 95_000, kind: 'gpu_thermal', worst: 86 }], trend: { days: Array.from({ length: 40 }, (_, i) => [`2026-09-${String((i % 30) + 1).padStart(2, '0')}`, 38 + i * 0.25 + rnd() * 2, 34 + i * 0.2]), cpu_rise: 9.5, gpu_rise: 7 } };
      case 'ping_get': pingSamples = [...pingSamples, rnd() > 0.97 ? null : 14 + rnd() * 9].slice(-120); return { host: a.host ?? '1.1.1.1', running: true, samples: pingSamples, avg: 17.8, jitter: 2.4, loss: 1 };
      case 'ping_gateway': return '192.168.0.1';
      case 'display_info': return {
        monitors: [
          { device: '\\\\.\\DISPLAY1', name: 'ASUS VG27AQ', adapter: 'NVIDIA GeForce RTX 4070', width: 2560, height: 1440, hz: 60, max_hz: 165, primary: true },
          { device: '\\\\.\\DISPLAY2', name: 'Dell P2419H', adapter: 'NVIDIA GeForce RTX 4070', width: 1920, height: 1080, hz: 60, max_hz: 60, primary: false },
        ], windowed_opt: true, vrr: false, timer: [15.6, 0.5], core_parking: 10,
      };
      case 'ram_purge': return 6.2 * GB;
      case 'battery_info': return { battery: { present: true, name: 'SMP 5B10W51867', design: 57000, full: 46800, cycles: 312, wear: 17.9, charge: 84, charging: true }, history: Array.from({ length: 30 }, (_, i) => [now - (30 - i) * 7 * 86400, 51000 - i * 140, 57000]) };
      case 'crashes_list': return [
        { at: now - 3600 * 5, kind: 'app_crash', app: 'FortniteClient-Win64-Shipping.exe', module: 'nvwgf2umx.dll', culprit: 'NVIDIA graphics driver', code: 'c0000005' },
        { at: now - 86400 * 2, kind: 'bsod', app: '', module: '', culprit: 'VIDEO_TDR_FAILURE (graphics driver stopped responding)', code: '0x116' },
        { at: now - 86400 * 2 - 30, kind: 'power_loss', app: '', module: '', culprit: null, code: '' },
        { at: now - 86400 * 4, kind: 'app_hang', app: 'explorer.exe', module: '', culprit: null, code: '' },
        { at: now - 86400 * 9, kind: 'app_crash', app: 'obs64.exe', module: 'KERNELBASE.dll', culprit: 'Windows core library (usually the app itself)', code: 'e0434352' },
      ];
      case 'security_status': return { defender: true, realtime: true, signature_age: 0, firewall: false, bitlocker: null, secure_boot: true, uac: true };
      case 'winupdate_status': return { last_install: now - 18 * 86400, last_title: 'KB5129195 Security Update', reboot_pending: true, service_running: false };
      case 'winupdate_pending': await new Promise(r => setTimeout(r, 1200)); return ['2026-10 Cumulative Update for Windows 11 (KB5131234)', 'Microsoft Defender Antivirus definition update'];
      case 'boot_info': return { last_boot: now - 9 * 3600, history: Array.from({ length: 14 }, (_, i) => [now - i * 86400, 38000 - i * 400 + rnd() * 6000, 19000 + rnd() * 3000, 15000]), slow: [['Docker Desktop', 8400], ['Steam Client', 5100], ['OneDrive', 2200]], needs_admin: false };
      case 'drivers_list': return [{ name: 'NVIDIA GeForce RTX 4070', vendor: 'nvidia', version: '32.0.15.6094', date: now - 160 * 86400, display_version: '560.94', download: 'https://www.nvidia.com/en-us/drivers/' }];
      case 'component_cleanup': await new Promise(r => setTimeout(r, 1500)); return 2.7 * GB;
      case 'stats_report': return {
        from: now - a.days * 86400, to: now, freed: 23.4 * GB,
        freed_by: [['junk', 9.1 * GB], ['care', 6.2 * GB], ['games', 5.8 * GB], ['privacy', 1.4 * GB], ['uninstaller', 0.9 * GB]],
        games: [['Counter-Strike 2', 410, 271], ['Fortnite', 220, 139], ['Hades II', 95, null]],
        care: Array.from({ length: 12 }, (_, i) => [now - (12 - i) * 86400 * 3, 58 + i * 2.6 + rnd() * 4]),
        daily_freed: Array.from({ length: a.days }, () => (rnd() > 0.4 ? rnd() * 5 * GB : 0)),
      };
      case 'settings_import': return {};
      case 'plugin:updater|check': return null;
      case 'plugin:dialog|open': return null;
      default:
        if (cmd.startsWith('plugin:')) return null;
        console.warn('[mock] unhandled command', cmd, a);
        return null;
    }
  }, { shouldMockEvents: true });
}
