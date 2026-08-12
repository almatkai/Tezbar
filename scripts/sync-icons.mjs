import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const root = path.join(__dirname, '..');

const srcTauriIcons = path.join(root, 'src-tauri', 'icons');
const ios1024 = path.join(srcTauriIcons, 'ios', 'AppIcon-512@2x.png');
const png512 = path.join(srcTauriIcons, 'icon.png');
const png256 = path.join(srcTauriIcons, '128x128@2x.png');
const png128 = path.join(srcTauriIcons, '128x128.png');
const png64 = path.join(srcTauriIcons, '64x64.png');
const png32 = path.join(srcTauriIcons, '32x32.png');
const ico = path.join(srcTauriIcons, 'icon.ico');
const icns = path.join(srcTauriIcons, 'icon.icns');

function copy(src, dest) {
  if (fs.existsSync(src)) {
    fs.mkdirSync(path.dirname(dest), { recursive: true });
    fs.copyFileSync(src, dest);
    console.log(`Copied ${path.relative(root, src)} -> ${path.relative(root, dest)}`);
  } else {
    console.warn(`Warning: source file not found ${src}`);
  }
}

// 1. Copy to build/
copy(ico, path.join(root, 'build', 'icon.ico'));
copy(icns, path.join(root, 'build', 'icon.icns'));
copy(png512, path.join(root, 'build', 'icon.png'));

// 2. Copy to resources/
copy(ico, path.join(root, 'resources', 'icon.ico'));
copy(icns, path.join(root, 'resources', 'icon.icns'));
copy(png512, path.join(root, 'resources', 'icon.png'));

// 3. Copy to resources/icons/
copy(ico, path.join(root, 'resources', 'icons', 'icon.ico'));
copy(icns, path.join(root, 'resources', 'icons', 'icon.icns'));
copy(png512, path.join(root, 'resources', 'icons', 'icon.png'));

// 4. Copy to AppIcons(3)/
copy(ios1024, path.join(root, 'AppIcons(3)', 'appstore.png'));
copy(png512, path.join(root, 'AppIcons(3)', 'playstore.png'));

const appiconset = path.join(root, 'AppIcons(3)', 'Assets.xcassets', 'AppIcon.appiconset');
copy(ios1024, path.join(appiconset, '1024.png'));
copy(png512, path.join(appiconset, '512.png'));
copy(png256, path.join(appiconset, '256.png'));
copy(png128, path.join(appiconset, '128.png'));
copy(png64, path.join(appiconset, '64.png'));
copy(png32, path.join(appiconset, '32.png'));

console.log('Successfully synced transparent icon files across all workspace locations!');
