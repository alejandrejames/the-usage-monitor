// LoginView.swift — WebView login screen (macOS + iOS shared)
// Loads claude.ai/login inside a WKWebView.
// AuthManager observes the cookie store and captures `sessionKey` automatically.

import SwiftUI
import WebKit

// MARK: - WKWebView representable

struct WebView: NSViewRepresentable {          // swap to UIViewRepresentable for iOS
    let url:         URL
    let authManager: AuthManager

    func makeNSView(context: Context) -> WKWebView {
        let config  = WKWebViewConfiguration()
        let webView = WKWebView(frame: .zero, configuration: config)
        webView.customUserAgent = "Mozilla/5.0 (Macintosh; Intel Mac OS X 15_0) AppleWebKit/537.36 Safari/537.36"
        authManager.attachCookieObserver(to: webView)
        webView.load(URLRequest(url: url))
        return webView
    }

    func updateNSView(_ nsView: WKWebView, context: Context) {}
}

// MARK: - LoginView

struct LoginView: View {
    let authManager: AuthManager
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        ZStack(alignment: .top) {

            // ── The actual claude.ai login page ──────────────────────────
            WebView(
                url: URL(string: "https://claude.ai/login")!,
                authManager: authManager
            )
            .clipShape(RoundedRectangle(cornerRadius: 14, style: .continuous))

            // ── Top header bar (Liquid Glass overlay) ─────────────────────
            HStack {
                Image(systemName: "waveform.and.sparkles")
                    .foregroundStyle(.purple)
                Text("Sign in to Claude")
                    .font(.system(size: 14, weight: .semibold))
                Spacer()
                Button("Cancel") { dismiss() }
                    .buttonStyle(.glass)
                    .font(.system(size: 12))
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 10)
            .claudeGlass()
            .padding(.horizontal, 12)
            .padding(.top, 10)
        }
        // Auto-dismiss when cookie is captured
        .onChange(of: authManager.isAuthenticated) { _, authenticated in
            if authenticated { dismiss() }
        }
    }
}
