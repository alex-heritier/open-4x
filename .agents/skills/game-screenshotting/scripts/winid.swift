// List on-screen windows with their CGWindowID and bounds, one per line:
//   <id>\t<owner>\t<title>\t<x>,<y>,<w>,<h>
// Used by window-shot.sh for `screencapture -l<id>`.
import CoreGraphics
import Foundation

let want = CommandLine.arguments.count > 1 ? CommandLine.arguments[1].lowercased() : ""
guard let list = CGWindowListCopyWindowInfo(
    [.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]]
else { exit(1) }

for window in list {
    guard (window[kCGWindowLayer as String] as? Int) == 0 else { continue }
    let owner = (window[kCGWindowOwnerName as String] as? String) ?? ""
    let title = (window[kCGWindowName as String] as? String) ?? ""
    if !want.isEmpty
        && !owner.lowercased().contains(want) && !title.lowercased().contains(want) {
        continue
    }
    let id = (window[kCGWindowNumber as String] as? Int) ?? 0
    let bounds = window[kCGWindowBounds as String] as? [String: Any] ?? [:]
    let rect = CGRect(dictionaryRepresentation: bounds as CFDictionary) ?? .zero
    let box = "\(Int(rect.origin.x)),\(Int(rect.origin.y)),\(Int(rect.width)),\(Int(rect.height))"
    print("\(id)\t\(owner)\t\(title)\t\(box)")
}
