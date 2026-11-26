cask "lantransport" do
  version "0.3.0"
  sha256 "82d84ca44833081588fc83e8f15a49a011bccaa36008407f7544fba23c8002d2"

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
