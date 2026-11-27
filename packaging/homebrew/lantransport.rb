cask "lantransport" do
  version "0.3.4"
  sha256 "f230783e243101b4538538c5313cbab8133ccd872ced2f1cc279965be423a22d"

  url "https://github.com/welpher/lantransport/releases/download/v#{version}/LanTransport_#{version}_universal.dmg"
  name "LanTransport"
  desc "Share files across your local network"
  homepage "https://github.com/welpher/lantransport"

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
