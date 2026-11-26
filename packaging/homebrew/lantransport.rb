cask "lantransport" do
  version "0.2.0"
  sha256 "23fb853bd7f74694ec49ca0b55d8b1c7644308a70d64e6572ba6c70783ce40f3"

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
