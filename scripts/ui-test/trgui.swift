// trgui - drive a macOS app with real input events, for UI testing.
//
// Compiled on demand by ./ui (into target/ui-test/bin). Coordinates are
// global screen points (the space of CGEvent and CGWindowList: the origin is
// the top left corner of the main display, y grows downwards, other displays
// can have negative coordinates). With --win <id>, they are relative to the
// top left corner of that window instead, and with --px they are pixels of a
// window screenshot (screencapture -o -l <id>), divided by the display's
// backing scale factor.
//
// Posting events needs the Accessibility permission, and window titles and
// screenshots need Screen Recording, for the process that runs this (the
// terminal, or whatever starts the shell).

import AppKit
import ApplicationServices
import CoreGraphics
import Foundation

func die(_ msg: String) -> Never {
    FileHandle.standardError.write((msg + "\n").data(using: .utf8)!)
    exit(1)
}

// MARK: windows

struct Win {
    let id: Int
    let pid: Int
    let owner: String
    let name: String
    let layer: Int
    let bounds: CGRect
}

func windows(pid: Int?, all: Bool = false) -> [Win] {
    let opts: CGWindowListOption = all ? [.optionAll] : [.optionOnScreenOnly, .excludeDesktopElements]
    guard let list = CGWindowListCopyWindowInfo(opts, kCGNullWindowID) as? [[String: Any]] else { return [] }
    return list.compactMap { d in
        let wpid = d[kCGWindowOwnerPID as String] as? Int ?? -1
        if let pid = pid, pid != wpid { return nil }
        guard let b = d[kCGWindowBounds as String] as? NSDictionary,
              let rect = CGRect(dictionaryRepresentation: b) else { return nil }
        return Win(id: d[kCGWindowNumber as String] as? Int ?? 0, pid: wpid,
                   owner: d[kCGWindowOwnerName as String] as? String ?? "",
                   name: d[kCGWindowName as String] as? String ?? "",
                   layer: d[kCGWindowLayer as String] as? Int ?? 0, bounds: rect)
    }
}

func window(_ id: Int) -> Win {
    guard let w = windows(pid: nil, all: true).first(where: { $0.id == id }) else { die("no window \(id)") }
    return w
}

/// The backing scale factor of the display that contains most of a rect
/// (given in CG coordinates)
func scale(for rect: CGRect) -> CGFloat {
    let mainHeight = NSScreen.screens.first?.frame.height ?? 0
    var best: (CGFloat, CGFloat) = (0, NSScreen.main?.backingScaleFactor ?? 2)
    for s in NSScreen.screens {
        // NSScreen frames have their origin at the bottom left of the main display
        let f = s.frame
        let cg = CGRect(x: f.minX, y: mainHeight - f.maxY, width: f.width, height: f.height)
        let i = cg.intersection(rect)
        let area = i.isNull ? 0 : i.width * i.height
        if area > best.0 { best = (area, s.backingScaleFactor) }
    }
    return best.1
}

// MARK: arguments

var args = Array(CommandLine.arguments.dropFirst())

func flag(_ name: String) -> Bool {
    if let i = args.firstIndex(of: name) { args.remove(at: i); return true }
    return false
}

func option(_ name: String) -> String? {
    guard let i = args.firstIndex(of: name), i + 1 < args.count else { return nil }
    let v = args[i + 1]
    args.removeSubrange(i...i + 1)
    return v
}

func number(_ s: String) -> CGFloat {
    guard let d = Double(s) else { die("not a number: \(s)") }
    return CGFloat(d)
}

/// Turns the x y arguments into a global point, following --win and --px
func point(_ x: String, _ y: String, win: Int?, px: Bool) -> CGPoint {
    var p = CGPoint(x: number(x), y: number(y))
    if let id = win {
        let w = window(id)
        if px {
            let s = scale(for: w.bounds)
            p = CGPoint(x: p.x / s, y: p.y / s)
        }
        p = CGPoint(x: w.bounds.minX + p.x, y: w.bounds.minY + p.y)
    } else if px {
        die("--px needs --win")
    }
    return p
}

// MARK: events

let source = CGEventSource(stateID: .hidSystemState)

func post(_ e: CGEvent?) {
    e?.post(tap: .cghidEventTap)
    usleep(8000)
}

func mouseMove(_ p: CGPoint) {
    post(CGEvent(mouseEventSource: source, mouseType: .mouseMoved, mouseCursorPosition: p, mouseButton: .left))
}

func click(_ p: CGPoint, right: Bool, count: Int, flags: CGEventFlags) {
    let (down, up, button): (CGEventType, CGEventType, CGMouseButton) =
        right ? (.rightMouseDown, .rightMouseUp, .right) : (.leftMouseDown, .leftMouseUp, .left)
    mouseMove(p)
    usleep(50000)
    for n in 1...count {
        for type in [down, up] {
            let e = CGEvent(mouseEventSource: source, mouseType: type, mouseCursorPosition: p, mouseButton: button)
            e?.setIntegerValueField(.mouseEventClickState, value: Int64(n))
            e?.flags = flags
            post(e)
        }
        usleep(60000)
    }
}

func drag(from a: CGPoint, to b: CGPoint, steps: Int) {
    mouseMove(a)
    usleep(50000)
    post(CGEvent(mouseEventSource: source, mouseType: .leftMouseDown, mouseCursorPosition: a, mouseButton: .left))
    usleep(150000)
    for i in 1...steps {
        let t = CGFloat(i) / CGFloat(steps)
        let p = CGPoint(x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t)
        post(CGEvent(mouseEventSource: source, mouseType: .leftMouseDragged, mouseCursorPosition: p, mouseButton: .left))
        usleep(20000)
    }
    usleep(150000)
    post(CGEvent(mouseEventSource: source, mouseType: .leftMouseUp, mouseCursorPosition: b, mouseButton: .left))
}

// US ANSI virtual key codes
let keyCodes: [String: CGKeyCode] = [
    "a": 0, "s": 1, "d": 2, "f": 3, "h": 4, "g": 5, "z": 6, "x": 7, "c": 8, "v": 9, "b": 11, "q": 12,
    "w": 13, "e": 14, "r": 15, "y": 16, "t": 17, "1": 18, "2": 19, "3": 20, "4": 21, "6": 22, "5": 23,
    "=": 24, "9": 25, "7": 26, "-": 27, "8": 28, "0": 29, "]": 30, "o": 31, "u": 32, "[": 33, "i": 34,
    "p": 35, "l": 37, "j": 38, "'": 39, "k": 40, ";": 41, "\\": 42, ",": 43, "/": 44, "n": 45, "m": 46,
    ".": 47, "`": 50,
    "return": 36, "enter": 36, "tab": 48, "space": 49, "delete": 51, "backspace": 51, "escape": 53,
    "esc": 53, "forwarddelete": 117, "home": 115, "end": 119, "pageup": 116, "pagedown": 121,
    "left": 123, "right": 124, "down": 125, "up": 126,
    "f1": 122, "f2": 120, "f3": 99, "f4": 118, "f5": 96, "f6": 97, "f7": 98, "f8": 100, "f9": 101,
    "f10": 109, "f11": 103, "f12": 111,
]

let modifiers: [String: (CGEventFlags, CGKeyCode)] = [
    "cmd": (.maskCommand, 55), "command": (.maskCommand, 55), "shift": (.maskShift, 56),
    "alt": (.maskAlternate, 58), "option": (.maskAlternate, 58), "ctrl": (.maskControl, 59),
    "control": (.maskControl, 59),
]

func parseFlags(_ s: String?) -> CGEventFlags {
    var flags: CGEventFlags = []
    for m in (s ?? "").split(separator: "+") where !m.isEmpty {
        guard let f = modifiers[m.lowercased()] else { die("unknown modifier \(m)") }
        flags.insert(f.0)
    }
    return flags
}

/// A key combination like cmd+shift+a, return or ctrl+tab
func key(_ combo: String) {
    let parts = combo.split(separator: "+").map { $0.lowercased() }
    guard let name = parts.last, let code = keyCodes[name] else { die("unknown key \(combo)") }
    var flags: CGEventFlags = []
    var mods: [CGKeyCode] = []
    for m in parts.dropLast() {
        guard let f = modifiers[m] else { die("unknown modifier \(m)") }
        flags.insert(f.0)
        mods.append(f.1)
    }
    for m in mods {
        let e = CGEvent(keyboardEventSource: source, virtualKey: m, keyDown: true)
        e?.flags = flags
        post(e)
    }
    for down in [true, false] {
        let e = CGEvent(keyboardEventSource: source, virtualKey: code, keyDown: down)
        e?.flags = flags
        post(e)
    }
    for m in mods.reversed() {
        post(CGEvent(keyboardEventSource: source, virtualKey: m, keyDown: false))
    }
    usleep(30000)
}

/// Types text. Characters of the US layout are sent with their real key
/// codes (with shift for capitals), because widgets that handle key presses
/// themselves (like the tree views' type-ahead) look at the key code and
/// not the text; other characters are sent as unicode text only, which is
/// enough for text entries.
func type(_ text: String) {
    for ch in text {
        let utf16 = Array(String(ch).utf16)
        let lower = String(ch).lowercased()
        let code = ch == " " ? keyCodes["space"] : keyCodes[lower]
        let shift = ch.isUppercase && code != nil
        if shift { post(CGEvent(keyboardEventSource: source, virtualKey: 56, keyDown: true)) }
        for down in [true, false] {
            let e = CGEvent(keyboardEventSource: source, virtualKey: code ?? 0, keyDown: down)
            if shift { e?.flags = .maskShift }
            e?.keyboardSetUnicodeString(stringLength: utf16.count, unicodeString: utf16)
            post(e)
        }
        if shift { post(CGEvent(keyboardEventSource: source, virtualKey: 56, keyDown: false)) }
        usleep(20000)
    }
}

// MARK: accessibility

func axAttr(_ e: AXUIElement, _ name: String) -> AnyObject? {
    var v: AnyObject?
    return AXUIElementCopyAttributeValue(e, name as CFString, &v) == .success ? v : nil
}

func axString(_ e: AXUIElement, _ name: String) -> String? {
    guard let v = axAttr(e, name) else { return nil }
    if let s = v as? String { return s.isEmpty ? nil : s }
    if let n = v as? NSNumber { return n.stringValue }
    return nil
}

func axFrame(_ e: AXUIElement) -> String {
    var p = CGPoint.zero, s = CGSize.zero
    if let v = axAttr(e, kAXPositionAttribute) { AXValueGetValue(v as! AXValue, .cgPoint, &p) }
    if let v = axAttr(e, kAXSizeAttribute) { AXValueGetValue(v as! AXValue, .cgSize, &s) }
    return "(\(Int(p.x)),\(Int(p.y)) \(Int(s.width))x\(Int(s.height)))"
}

func axDump(_ e: AXUIElement, depth: Int, maxDepth: Int) {
    var line = String(repeating: "  ", count: depth) + (axString(e, kAXRoleAttribute) ?? "?")
    for (label, attr) in [("title", kAXTitleAttribute), ("desc", kAXDescriptionAttribute),
                          ("value", kAXValueAttribute), ("label", "AXLabel"), ("id", "AXIdentifier")] {
        if let s = axString(e, attr) { line += " \(label)=\(s.debugDescription)" }
    }
    print(line + " " + axFrame(e))
    guard depth < maxDepth, let children = axAttr(e, kAXChildrenAttribute) as? [AXUIElement] else { return }
    for c in children { axDump(c, depth: depth + 1, maxDepth: maxDepth) }
}

/// Presses an item of the app's (native) menu bar, by the titles of the
/// menus and items, e.g. "File" "Connect" "UI test"
func axMenu(pid: Int32, path: [String]) {
    var e: AXUIElement = AXUIElementCreateApplication(pid)
    guard let bar = axAttr(e, kAXMenuBarAttribute) else { die("the app has no menu bar") }
    e = bar as! AXUIElement
    for title in path {
        // menu bar items and menu items have their items in an AXMenu child
        var candidates = axAttr(e, kAXChildrenAttribute) as? [AXUIElement] ?? []
        if candidates.count == 1, axString(candidates[0], kAXRoleAttribute) == "AXMenu" {
            candidates = axAttr(candidates[0], kAXChildrenAttribute) as? [AXUIElement] ?? []
        }
        guard let next = candidates.first(where: { axString($0, kAXTitleAttribute) == title }) else {
            let titles = candidates.compactMap { axString($0, kAXTitleAttribute) }
            die("no menu item \(title.debugDescription), there are: \(titles)")
        }
        e = next
    }
    let err = AXUIElementPerformAction(e, kAXPressAction as CFString)
    if err != .success { die("pressing the menu item failed: \(err.rawValue)") }
}

// MARK: commands

let usage = """
usage: trgui <command> [options]
  check                              permissions (Accessibility, Screen Recording)
  windows [--pid P] [--all]          list windows: id, bounds in points, scale, name
  frontmost --pid P                  bring the app to the front
  move X Y                           move the pointer
  click X Y [--right] [--count N] [--mods cmd+shift]
  drag X1 Y1 X2 Y2 [--steps N]
  scroll DY [DX] [--at X Y]           scroll by lines (positive is up)
  key COMBO...                       e.g. return, cmd+a, shift+f10, ctrl+tab
  type TEXT                          type text
  ax --pid P [--depth N]             dump the accessibility tree
  menu --pid P TITLE...              press a menu bar item, e.g. File Quit
  scale --win W                      backing scale of the window's display
  Coordinates: global points, or with --win W relative to that window,
  and with --px in pixels of the window's screenshot.
"""

guard !args.isEmpty else { print(usage); exit(2) }
let cmd = args.removeFirst()
let win = option("--win").map { Int($0) ?? -1 }
let px = flag("--px")

switch cmd {
case "check":
    let ax = AXIsProcessTrusted()
    let sr = CGPreflightScreenCaptureAccess()
    print("accessibility (post events, read AX tree): \(ax ? "granted" : "MISSING")")
    print("screen recording (screenshots, window titles): \(sr ? "granted" : "MISSING")")
    let session = CGSessionCopyCurrentDictionary() as? [String: Any] ?? [:]
    let locked = session["CGSSessionScreenIsLocked"] as? Bool ?? false
    let onConsole = session[kCGSessionOnConsoleKey as String] as? Bool ?? true
    print("screen: \(locked ? "LOCKED (screenshots are black, input goes nowhere)" : onConsole ? "unlocked" : "NOT ON CONSOLE")")
    exit(ax && sr && !locked && onConsole ? 0 : 1)
case "windows":
    let pid = option("--pid").flatMap { Int($0) }
    let all = flag("--all")
    for w in windows(pid: pid, all: all) where all || w.layer == 0 || pid != nil {
        let b = w.bounds
        print("\(w.id)\tpid=\(w.pid)\tlayer=\(w.layer)\t\(Int(b.minX)),\(Int(b.minY)) \(Int(b.width))x\(Int(b.height))\tscale=\(scale(for: b))\t\(w.owner)\t\(w.name.debugDescription)")
    }
case "frontmost":
    guard let pid = option("--pid").flatMap({ Int32($0) }),
          let app = NSRunningApplication(processIdentifier: pid) else { die("no such app") }
    app.unhide()
    if !app.activate(options: [.activateAllWindows]) { die("activation refused") }
    usleep(300000)
case "scale":
    guard let id = win else { die("--win needed") }
    print(scale(for: window(id).bounds))
case "move":
    guard args.count >= 2 else { die(usage) }
    mouseMove(point(args[0], args[1], win: win, px: px))
case "click":
    let right = flag("--right")
    let count = option("--count").flatMap { Int($0) } ?? 1
    let flags = parseFlags(option("--mods"))
    guard args.count >= 2 else { die(usage) }
    click(point(args[0], args[1], win: win, px: px), right: right, count: count, flags: flags)
case "drag":
    let steps = option("--steps").flatMap { Int($0) } ?? 20
    guard args.count >= 4 else { die(usage) }
    drag(from: point(args[0], args[1], win: win, px: px), to: point(args[2], args[3], win: win, px: px), steps: steps)
case "scroll":
    if let i = args.firstIndex(of: "--at"), i + 2 < args.count {
        mouseMove(point(args[i + 1], args[i + 2], win: win, px: px))
        args.removeSubrange(i...i + 2)
        usleep(50000)
    }
    guard let dy = args.first.flatMap({ Int32($0) }) else { die(usage) }
    let dx = args.count > 1 ? Int32(args[1]) ?? 0 : 0
    post(CGEvent(scrollWheelEvent2Source: source, units: .line, wheelCount: 2, wheel1: dy, wheel2: dx, wheel3: 0))
case "key":
    for k in args { key(k) }
case "type":
    type(args.joined(separator: " "))
case "ax":
    guard let pid = option("--pid").flatMap({ Int32($0) }) else { die("--pid needed") }
    let depth = option("--depth").flatMap { Int($0) } ?? 12
    if !AXIsProcessTrusted() { die("the Accessibility permission is missing") }
    axDump(AXUIElementCreateApplication(pid), depth: 0, maxDepth: depth)
case "menu":
    guard let pid = option("--pid").flatMap({ Int32($0) }) else { die("--pid needed") }
    if !AXIsProcessTrusted() { die("the Accessibility permission is missing") }
    axMenu(pid: pid, path: args)
default:
    die(usage)
}
