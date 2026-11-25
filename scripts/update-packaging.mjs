import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";

const tag = process.argv[2] ?? process.env.RELEASE_TAG;
if (!tag) {
  console.error("usage: node scripts/update-packaging.mjs v0.1.1");
  process.exit(1);
}
const version = tag.replace(/^v/, "");
const repo = process.env.GITHUB_REPOSITORY ?? "welpher/lantransport";
const release = JSON.parse(
  execFileSync("gh", ["api", `repos/${repo}/releases/tags/${tag}`], { encoding: "utf8" }),
);

const asset = (suffix) => {
  const found = release.assets.find((entry) => entry.name.endsWith(suffix));
  if (!found) {
    throw new Error(`release ${tag} has no asset ending in ${suffix}`);
  }
  return {
    name: found.name,
    url: found.browser_download_url,
    sha256: found.digest.replace(/^sha256:/, ""),
  };
};

const dmg = asset("_universal.dmg");
const exe = asset("_x64-setup.exe");
const appimage = asset("_amd64.AppImage");

const replace = (path, pairs) => {
  let text = readFileSync(path, "utf8");
  for (const [pattern, value] of pairs) {
    if (!pattern.test(text)) {
      throw new Error(`pattern ${pattern} not found in ${path}`);
    }
    text = text.replace(pattern, value);
  }
  writeFileSync(path, text);
};

replace("packaging/winget/welpher.LanTransport.yaml", [
  [/^PackageVersion: .*$/m, `PackageVersion: ${version}`],
]);
replace("packaging/winget/welpher.LanTransport.installer.yaml", [
  [/^PackageVersion: .*$/m, `PackageVersion: ${version}`],
  [/^    InstallerUrl: .*$/m, `    InstallerUrl: ${exe.url}`],
  [/^    InstallerSha256: .*$/m, `    InstallerSha256: ${exe.sha256.toUpperCase()}`],
]);
replace("packaging/winget/welpher.LanTransport.locale.en-US.yaml", [
  [/^PackageVersion: .*$/m, `PackageVersion: ${version}`],
]);
replace("packaging/aur/PKGBUILD", [
  [/^pkgver=.*$/m, `pkgver=${version}`],
  [/^sha256sums=\('.*'\)$/m, `sha256sums=('${appimage.sha256}')`],
]);

const cask = `cask "lantransport" do
  version "${version}"
  sha256 "${dmg.sha256}"

  url "https://github.com/${repo}/releases/download/v#{version}/LanTransport_#{version}_universal.dmg"
  name "LanTransport"
  desc "Share files across your local network"
  homepage "https://github.com/${repo}"

  livecheck do
    url :url
    strategy :github_latest
  end

  app "LanTransport.app"

  zap trash: [
    "~/Library/Application Support/app.lantransport.desktop",
    "~/Library/Caches/app.lantransport.desktop",
  ]
end
`;
writeFileSync("packaging/homebrew/lantransport.rb", cask);

console.log(`updated manifests for ${tag}`);
console.log(`  dmg      ${dmg.name} ${dmg.sha256}`);
console.log(`  exe      ${exe.name} ${exe.sha256}`);
console.log(`  appimage ${appimage.name} ${appimage.sha256}`);
