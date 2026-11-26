cask "lantransport" do
  version "0.3.1"
  sha256 "c364af639df43a16953c98701cc8c7bac5f251a23adc3c6b1f240f222cc93f46"

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
