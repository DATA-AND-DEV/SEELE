// Diz se a sessão gráfica está bloqueada: com ela bloqueada, a janela não está
// no palco e a árvore de acessibilidade das janelas não responde.
import CoreGraphics
let d = CGSessionCopyCurrentDictionary() as? [String: Any] ?? [:]
print("bloqueada=\(d["CGSSessionScreenIsLocked"] as? Bool ?? false) noConsole=\(d["kCGSSessionOnConsoleKey"] as? Bool ?? false)")
