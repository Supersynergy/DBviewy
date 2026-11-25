/** Stage a local app with source-bound identity. Does not install or restart it. */
import {existsSync, readFileSync, mkdirSync} from 'node:fs';
import {resolve, join, dirname} from 'node:path';
const [binaryArg, baseArg, outputArg, build = new Date().toISOString().slice(0,10).replaceAll('-','') + '.1'] = Bun.argv.slice(2);
if (!binaryArg || !baseArg || !outputArg) throw new Error('Usage: bun pkg/macos/bundle.ts BINARY BASE_APP NEW_APP [BUILD]');
const binary = resolve(binaryArg), base = resolve(baseArg), output = resolve(outputArg);
if (!existsSync(binary) || !existsSync(join(base,'Contents/Info.plist'))) throw new Error('Missing binary or base app');
if (existsSync(output) || !output.endsWith('.app')) throw new Error('Output must be a new .app path');
const repo = resolve(import.meta.dir, '../..');
const version = readFileSync(join(repo,'Cargo.toml'),'utf8').match(/^version = "([^"]+)"/m)?.[1];
if (!version || !/^\d+(\.\d+){1,2}$/.test(build)) throw new Error('Invalid package version or numeric bundle build');
function run(args: string[]): void {
 const r = Bun.spawnSync(args, {stdout:'pipe', stderr:'pipe'});
 if (r.exitCode !== 0) throw new Error(`${args[0]}: ${r.stderr.toString()}`);
}
run(['ditto', base, output]);
run(['cp', binary, join(output,'Contents/MacOS/octa')]);
run(['cp', join(repo,'pkg/macos/Info.plist'), join(output,'Contents/Info.plist')]);
const plist = join(output,'Contents/Info.plist');
run(['plutil','-replace','CFBundleShortVersionString','-string',version.split('-')[0]!,plist]);
run(['plutil','-replace','CFBundleVersion','-string',build,plist]);
const icons = join(dirname(output),'dbviewy-' + build + '.iconset');
if (existsSync(icons)) throw new Error('Icon staging path already exists');
mkdirSync(icons);
for (const size of [16,32,128,256,512]) for (const scale of [1,2]) {
 const target = join(icons,`icon_${size}x${size}${scale===2?'@2x':''}.png`);
 run(['sips','-z',String(size*scale),String(size*scale),join(repo,'assets/dbviewy.png'),'--out',target]);
}
run(['iconutil','-c','icns',icons,'-o',join(output,'Contents/Resources/dbviewy.icns')]);
run(['codesign','--force','--deep','--sign','-',output]);
run(['codesign','--verify','--deep','--strict',output]);
console.log(JSON.stringify({app:output, version, build, binarySha256:new Bun.CryptoHasher('sha256').update(readFileSync(join(output,'Contents/MacOS/octa'))).digest('hex')}));
