// M3 da 0.16 — dirigir a janela do SEELE pela acessibilidade, sem mover o
// mouse de quem está na máquina.
//
// Não é código de produto. Compila com `swiftc -O ax.swift -o ax`.
//
//   ax janelas <pid>                    id, posição e tamanho das janelas
//   ax arvore <pid> [fundura]           papel | título | descrição | valor
//   ax apertar <pid> <texto> [n]        AXPress no n-ésimo (0) elemento cujo
//                                       título, descrição ou valor contém o texto
//   ax escrever <pid> <texto> <valor>   põe o valor no campo que casa o texto
//   ax textos <pid> <texto>             valores dos textos que contêm o texto
//   ax arrastar <x0> <y0> <x1> <y1> <ms> arraste de verdade (CGEvent), em
//                                       coordenadas de tela — move o mouse
import ApplicationServices
import Cocoa

func attr(_ e: AXUIElement, _ name: String) -> AnyObject? {
    var v: AnyObject?
    return AXUIElementCopyAttributeValue(e, name as CFString, &v) == .success ? v : nil
}
func str(_ e: AXUIElement, _ name: String) -> String {
    if let s = attr(e, name) as? String { return s }
    if let n = attr(e, name) as? NSNumber { return n.stringValue }
    return ""
}
func filhos(_ e: AXUIElement) -> [AXUIElement] {
    (attr(e, kAXChildrenAttribute) as? [AXUIElement]) ?? []
}
func janelasDo(_ pid: Int32) -> [AXUIElement] {
    let app = AXUIElementCreateApplication(pid)
    // O WebKit só expõe a árvore da página depois de alguém pedir.
    AXUIElementSetAttributeValue(app, "AXEnhancedUserInterface" as CFString, kCFBooleanTrue)
    AXUIElementSetAttributeValue(app, "AXManualAccessibility" as CFString, kCFBooleanTrue)
    return (attr(app, kAXWindowsAttribute) as? [AXUIElement]) ?? []
}
func percorrerJanelas(_ pid: Int32, _ max: Int, _ f: (AXUIElement, Int) -> Bool) {
    for w in janelasDo(pid) { if percorrer(w, 0, max, f) { return } }
}
func percorrer(_ e: AXUIElement, _ fundura: Int, _ max: Int, _ f: (AXUIElement, Int) -> Bool) -> Bool {
    if f(e, fundura) { return true }
    if fundura >= max { return false }
    for c in filhos(e) { if percorrer(c, fundura + 1, max, f) { return true } }
    return false
}
func descreve(_ e: AXUIElement) -> String {
    let partes = [str(e, kAXRoleAttribute), str(e, kAXTitleAttribute),
                  str(e, kAXDescriptionAttribute), str(e, kAXValueAttribute)]
    return partes.map { $0.replacingOccurrences(of: "\n", with: " ") }.joined(separator: " | ")
}
func casa(_ e: AXUIElement, _ texto: String) -> Bool {
    let t = texto.lowercased()
    for n in [kAXTitleAttribute, kAXDescriptionAttribute, kAXValueAttribute] {
        if str(e, n).lowercased().contains(t) { return true }
    }
    return false
}

let a = CommandLine.arguments
guard a.count >= 2 else { print("uso: ver o cabeçalho"); exit(2) }
switch a[1] {
case "janelas":
    let pid = Int32(a[2])!
    let lista = CGWindowListCopyWindowInfo([.optionAll], kCGNullWindowID) as! [[String: Any]]
    for w in lista where (w[kCGWindowOwnerPID as String] as? Int32) == pid {
        let b = w[kCGWindowBounds as String] as? [String: Any] ?? [:]
        print(w[kCGWindowNumber as String] ?? "", w[kCGWindowLayer as String] ?? "",
              b["X"] ?? "", b["Y"] ?? "", b["Width"] ?? "", b["Height"] ?? "",
              w[kCGWindowName as String] ?? "")
    }
case "estado":
    for w in janelasDo(Int32(a[2])!) {
        var p = CGPoint.zero, t = CGSize.zero
        if let v = attr(w, kAXPositionAttribute) { AXValueGetValue(v as! AXValue, .cgPoint, &p) }
        if let v = attr(w, kAXSizeAttribute) { AXValueGetValue(v as! AXValue, .cgSize, &t) }
        print("janela", Int(p.x), Int(p.y), Int(t.width), Int(t.height),
              "minimizada=" + str(w, kAXMinimizedAttribute), "cheia=" + str(w, "AXFullScreen"))
    }
case "numero":
    // O número da janela no servidor de janelas, para o `screencapture -l`.
    typealias F = @convention(c) (AXUIElement, UnsafeMutablePointer<CGWindowID>) -> AXError
    let h = dlopen("/System/Library/Frameworks/ApplicationServices.framework/ApplicationServices", RTLD_NOW)
    let f = unsafeBitCast(dlsym(h, "_AXUIElementGetWindow"), to: F.self)
    for w in janelasDo(Int32(a[2])!) { var n: CGWindowID = 0; _ = f(w, &n); print(n) }
case "ativar":
    // Traz a janela ao palco (Stage Manager) e ao primeiro plano: uma janela
    // encoberta tem o desenho estrangulado pelo WebKit, e o tempo de quadro
    // medido nela não diria nada.
    let pid = Int32(a[2])!
    NSRunningApplication(processIdentifier: pid)?.activate()
    for w in janelasDo(pid) { AXUIElementPerformAction(w, kAXRaiseAction as CFString) }
    print("ok")
case "caixas":
    // papel, posição e tamanho de todo elemento entre as larguras pedidas
    let pid = Int32(a[2])!, wmin = Double(a[3])!, wmax = Double(a[4])!
    percorrerJanelas(pid, 70) { e, d in
        var p = CGPoint.zero, t = CGSize.zero
        if let v = attr(e, kAXPositionAttribute) { AXValueGetValue(v as! AXValue, .cgPoint, &p) }
        if let v = attr(e, kAXSizeAttribute) { AXValueGetValue(v as! AXValue, .cgSize, &t) }
        if Double(t.width) >= wmin && Double(t.width) <= wmax {
            print(d, str(e, kAXRoleAttribute), str(e, kAXSubroleAttribute), Int(p.x), Int(p.y), Int(t.width), Int(t.height), str(e, kAXDescriptionAttribute))
        }
        return false
    }
case "rolar":
    // AXScrollToVisible no primeiro elemento cujo texto casa
    let alvo = a[3]; var feito = false
    percorrerJanelas(Int32(a[2])!, 70) { e, _ in
        guard casa(e, alvo) else { return false }
        let r = AXUIElementPerformAction(e, "AXScrollToVisible" as CFString)
        print("rolado:", descreve(e), "→", r.rawValue); feito = true; return true
    }
    if !feito { print("não achei:", alvo); exit(1) }
case "laranja":
    // ax laranja <png da janela> <x da janela> <y da janela> <x0> <y0> <x1> <y1>
    // O centro, em pontos de tela, do primeiro bloco de 8 × 8 pixels de #f2521f
    // (a peça da MESA) dentro do retângulo de tela pedido. Linhas e bordas de
    // 1 a 3 pixels não fecham um bloco desses; um círculo de raio 10 fecha.
    let url = URL(fileURLWithPath: a[2])
    guard let fonte = CGImageSourceCreateWithURL(url as CFURL, nil),
          let img = CGImageSourceCreateImageAtIndex(fonte, 0, nil) else { print("sem imagem"); exit(1) }
    let wx = Double(a[3])!, wy = Double(a[4])!
    let x0 = Double(a[5])!, y0 = Double(a[6])!, x1 = Double(a[7])!, y1 = Double(a[8])!
    let w = img.width, h = img.height
    let ctx = CGContext(data: nil, width: w, height: h, bitsPerComponent: 8, bytesPerRow: w * 4,
                        space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    ctx.draw(img, in: CGRect(x: 0, y: 0, width: w, height: h))
    let px = ctx.data!.bindMemory(to: UInt8.self, capacity: w * h * 4)
    // a captura sai em pixels (2× num Retina); a escala é a largura da imagem
    // sobre a da janela em pontos, que vem do `ax estado`
    let escala = Double(a.count > 9 ? a[9] : "2")!
    func laranja(_ x: Int, _ y: Int) -> Bool {
        let i = (y * w + x) * 4
        let r = Int(px[i]), g = Int(px[i + 1]), b = Int(px[i + 2])
        return abs(r - 0xf2) < 28 && abs(g - 0x52) < 28 && abs(b - 0x1f) < 28
    }
    let ix0 = max(0, Int((x0 - wx) * escala)), iy0 = max(0, Int((y0 - wy) * escala))
    let ix1 = min(w - 9, Int((x1 - wx) * escala)), iy1 = min(h - 9, Int((y1 - wy) * escala))
    var achou = false
    if ix0 < ix1 && iy0 < iy1 {
        busca: for y in iy0..<iy1 { for x in ix0..<ix1 {
            var cheio = true
            teste: for dy in 0..<8 { for dx in 0..<8 where !laranja(x + dx, y + dy) { cheio = false; break teste } }
            if cheio {
                print(Int(wx + Double(x + 4) / escala), Int(wy + Double(y + 4) / escala))
                achou = true; break busca
            }
        } }
    }
    if !achou { print("sem peça"); exit(1) }
case "desminimizar":
    for w in janelasDo(Int32(a[2])!) {
        AXUIElementSetAttributeValue(w, kAXMinimizedAttribute as CFString, kCFBooleanFalse)
    }
    print("ok")
case "arvore":
    let max = a.count > 3 ? Int(a[3])! : 40
    percorrerJanelas(Int32(a[2])!, max) { e, d in
        let s = descreve(e)
        if s.replacingOccurrences(of: " | ", with: "").count > 0 {
            print(String(repeating: " ", count: d) + s)
        }
        return false
    }
case "apertar":
    let alvo = a[3]; var n = a.count > 4 ? Int(a[4])! : 0
    var feito = false
    percorrerJanelas(Int32(a[2])!, 60) { e, _ in
        let papel = str(e, kAXRoleAttribute)
        guard ["AXButton", "AXLink", "AXRadioButton", "AXCheckBox", "AXMenuItem", "AXPopUpButton", "AXTab"].contains(papel),
              casa(e, alvo) else { return false }
        if n > 0 { n -= 1; return false }
        let r = AXUIElementPerformAction(e, kAXPressAction as CFString)
        print("apertado:", descreve(e), "→", r.rawValue)
        feito = true
        return true
    }
    if !feito { print("não achei:", alvo); exit(1) }
case "escrever":
    let alvo = a[3], valor = a[4]
    var feito = false
    percorrerJanelas(Int32(a[2])!, 60) { e, _ in
        guard ["AXTextField", "AXTextArea"].contains(str(e, kAXRoleAttribute)), casa(e, alvo) else { return false }
        AXUIElementSetAttributeValue(e, kAXFocusedAttribute as CFString, kCFBooleanTrue)
        let r = AXUIElementSetAttributeValue(e, kAXValueAttribute as CFString, valor as CFString)
        print("escrito:", descreve(e), "→", r.rawValue)
        feito = true
        return true
    }
    if !feito { print("não achei:", alvo); exit(1) }
case "textos":
    let alvo = a[3]
    // A fundura é opcional, 60 como sempre. A carga-cartoes (10/10) pede mais:
    // a lista com 251 cartões e a linha de uma contribuição dentro de uma
    // mensagem moram mais fundo, e «não achei» precisa querer dizer «não está».
    let fundura = a.count > 4 ? Int(a[4])! : 60
    percorrerJanelas(Int32(a[2])!, fundura) { e, _ in
        if casa(e, alvo) { print(descreve(e)) }
        return false
    }
case "arrastar":
    let x0 = Double(a[2])!, y0 = Double(a[3])!, x1 = Double(a[4])!, y1 = Double(a[5])!
    let ms = Double(a[6])!
    let passos = max(10, Int(ms / 8))
    let fonte = CGEventSource(stateID: .combinedSessionState)
    func ev(_ t: CGEventType, _ x: Double, _ y: Double) {
        CGEvent(mouseEventSource: fonte, mouseType: t, mouseCursorPosition: CGPoint(x: x, y: y),
                mouseButton: .left)?.post(tap: .cgSessionEventTap)
    }
    ev(.leftMouseDown, x0, y0)
    for i in 1...passos {
        let f = Double(i) / Double(passos)
        // ida e volta, para o arraste durar `ms` sem sair da tela
        let g = f < 0.5 ? f * 2 : (1 - f) * 2
        ev(.leftMouseDragged, x0 + (x1 - x0) * g, y0 + (y1 - y0) * g)
        usleep(useconds_t(ms * 1000 / Double(passos)))
    }
    ev(.leftMouseUp, x0, y0)
    print("arrastado", passos, "passos")
default:
    print("comando desconhecido"); exit(2)
}
