cask "lantransport" do
  version "0.3.3"
  sha256 "85bddcfecf7b38db09157b476670081357060d716676871b56d53c202a9d45d9"

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
