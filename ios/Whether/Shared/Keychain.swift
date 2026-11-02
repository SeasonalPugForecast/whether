import Foundation
import Security

/// Each provider's API key in the Keychain (this device only; never in files or
/// logs), one item per provider with `account = provider id`. The
/// `openweather` account is the one earlier versions already used.
enum Keychain {
    private static let service = "dev.savery.whether"

    private static func query(_ provider: String) -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: provider,
        ]
    }

    /// The stored key for `provider` (a source id such as "openweather"), if any.
    static func key(for provider: String) -> String? {
        var item: CFTypeRef?
        var q = query(provider)
        q[kSecReturnData as String] = true
        q[kSecMatchLimit as String] = kSecMatchLimitOne
        guard SecItemCopyMatching(q as CFDictionary, &item) == errSecSuccess,
            let data = item as? Data
        else { return nil }
        return String(data: data, encoding: .utf8)
    }

    /// Stores `key` for `provider`, or removes it when empty. Readable after the
    /// first unlock, so background refreshes find it.
    static func setKey(_ key: String, for provider: String) {
        SecItemDelete(query(provider) as CFDictionary)
        let trimmed = key.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        var q = query(provider)
        q[kSecValueData as String] = Data(trimmed.utf8)
        q[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        SecItemAdd(q as CFDictionary, nil)
    }

    /// The keys saved for `providers`, by provider id (those without a key are left out).
    static func keys(for providers: [String]) -> [String: String] {
        var out: [String: String] = [:]
        for provider in providers {
            if let key = key(for: provider)?.trimmingCharacters(in: .whitespacesAndNewlines), !key.isEmpty {
                out[provider] = key
            }
        }
        return out
    }
}
