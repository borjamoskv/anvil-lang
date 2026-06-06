#!/usr/bin/env python3
# [C5-REAL] — Industrial Noir Overlay — Sovereign UI
import signal
from AppKit import *
from WebKit import *

HTML_TEMPLATE = """
<!DOCTYPE html>
<html>
<head>
    <style>
        body {
            margin: 0;
            padding: 0;
            background: rgba(10, 10, 10, 0.9);
            color: #2B3BE5;
            font-family: 'Helvetica Neue', sans-serif;
            overflow: hidden;
            border: 1px solid #2B3BE5;
            box-sizing: border-box;
            height: 100vh;
            display: flex;
            flex-direction: column;
            justify-content: center;
            align-items: center;
            backdrop-filter: blur(40px);
        }
        .container {
            text-align: center;
            border: 1px solid #2B3BE5;
            padding: 4rem;
            position: relative;
        }
        .container::before {
            content: '';
            position: absolute;
            top: -5px; left: -5px;
            width: 20px; height: 20px;
            border-top: 2px solid #2B3BE5;
            border-left: 2px solid #2B3BE5;
        }
        .container::after {
            content: '';
            position: absolute;
            bottom: -5px; right: -5px;
            width: 20px; height: 20px;
            border-bottom: 2px solid #2B3BE5;
            border-right: 2px solid #2B3BE5;
        }
        .status {
            font-size: 5rem;
            font-weight: 900;
            text-transform: uppercase;
            letter-spacing: 1.5rem;
            text-shadow: 0 0 20px #FF9F1C, 2px 2px #2B3BE5;
            margin-bottom: 0.5rem;
            position: relative;
            animation: glitch 3s infinite;
        }
        @keyframes glitch {
            0% { transform: translate(0) }
            20% { transform: translate(-2px, 2px) }
            40% { transform: translate(-2px, -2px) }
            60% { transform: translate(2px, 2px) }
            80% { transform: translate(2px, -2px) }
            100% { transform: translate(0) }
        }
        .status::before, .status::after {
            content: 'MAESTRO';
            position: absolute;
            top: 0;
            left: 0;
            width: 100%;
            height: 100%;
            background: transparent;
        }
        .status::before {
            left: 2px;
            text-shadow: -2px 0 red;
            clip: rect(24px, 550px, 90px, 0);
            animation: glitch-anim 2s infinite linear alternate-reverse;
        }
        .status::after {
            left: -2px;
            text-shadow: -2px 0 blue;
            clip: rect(85px, 550px, 140px, 0);
            animation: glitch-anim 2.5s infinite linear alternate-reverse;
        }
        @keyframes glitch-anim {
            0% { clip: rect(10px, 9999px, 80px, 0); }
            20% { clip: rect(60px, 9999px, 12px, 0); }
            40% { clip: rect(30px, 9999px, 50px, 0); }
            60% { clip: rect(80px, 9999px, 20px, 0); }
            80% { clip: rect(40px, 9999px, 70px, 0); }
            100% { clip: rect(90px, 9999px, 10px, 0); }
        }
        .sub {
            font-size: 1rem;
            letter-spacing: 0.5rem;
            opacity: 0.9;
            color: #FF9F1C;
        }
        .telemetry {
            margin-top: 3rem;
            font-family: monospace;
            font-size: 0.7rem;
            line-height: 1.5;
            text-align: left;
            opacity: 0.6;
        }
    </style>
</head>
<body>
    <div class="container">
        <div class="status">MAESTRO</div>
        <div class="sub">PHYSICAL SOVEREIGNTY v9.0.0</div>
        <div class="telemetry">
            ∴ STATE: C5-REAL<br>
            ∴ PORT: 9229 (CDP)<br>
            ∴ AX-Ω: SILICON_BYPASS<br>
            ∴ B-OS: MACOS_SUR_SUR
        </div>
    </div>
</body>
</html>
"""

class NoirOverlay(NSObject):
    def run(self):
        self.app = NSApplication.sharedApplication()
        # Ensure it can show up on top of full-screen apps
        self.app.setActivationPolicy_(NSApplicationActivationPolicyAccessory)
        
        screen = NSScreen.mainScreen().frame()
        self.win = NSWindow.alloc().initWithContentRect_styleMask_backing_defer_(
            screen,
            NSWindowStyleMaskBorderless,
            NSBackingStoreBuffered,
            False
        )
        self.win.setOpaque_(False)
        self.win.setBackgroundColor_(NSColor.clearColor())
        self.win.setLevel_(NSStatusWindowLevel + 2)
        self.win.setCollectionBehavior_(
            NSWindowCollectionBehaviorCanJoinAllSpaces | 
            NSWindowCollectionBehaviorFullScreenAuxiliary
        )
        self.win.setIgnoresMouseEvents_(True)
        
        config = WKWebViewConfiguration.alloc().init()
        self.webview = WKWebView.alloc().initWithFrame_configuration_(
            self.win.contentView().bounds(),
            config
        )
        self.webview.setBackgroundColor_(NSColor.clearColor())
        self.webview.setOpaque_(False)
        self.win.setContentView_(self.webview)
        
        self.webview.loadHTMLString_baseURL_(HTML_TEMPLATE, None)
        self.win.makeKeyAndOrderFront_(None)
        
        # Shutdown signal
        signal.signal(signal.SIGINT, lambda s, f: self.app.terminate_(None))
        self.app.run()

if __name__ == "__main__":
    overlay = NoirOverlay.alloc().init()
    overlay.run()
