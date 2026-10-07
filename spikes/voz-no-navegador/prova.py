#!/usr/bin/env python3
"""A prova de bancada do spike: dois Chromium numa sala, um falando.

Ela não substitui o celular — é o contrário: prova que a página e o servidor
funcionam **antes** de alguém levar um iPhone para a mesa, para que um defeito
no celular seja do celular e não do spike.

O que ela cobra:

1. Os dois abrem o WebTransport com o hash do certificado autoassinado.
2. A voz de A chega a B, decodificada, e toca sem faltas contínuas.
3. O eco de A volta e mede a ida e volta da voz (captura → Opus → rede → decode).
4. Os relatórios chegam ao servidor (`relatorios.jsonl`).
5. **Um hash adulterado é recusado.** Sem este, os quatro de cima passariam
   igual se o navegador estivesse aceitando qualquer certificado — e então a
   prova não diria nada sobre o `serverCertificateHashes`.

Uso (do diretório do spike):
    cargo build --release && python3 prova.py
"""

import json
import pathlib
import subprocess
import sys
import tempfile
import time

from playwright.sync_api import sync_playwright

AQUI = pathlib.Path(__file__).resolve().parent
BINARIO = AQUI / "target" / "release" / "spike-voz-no-navegador"
PORTA, HTTP = 4434, 8081  # fora das de uso manual, para a prova não brigar com um servidor aberto
URL = f"http://localhost:{HTTP}/"
FALANDO_S = 10


def estado(pagina):
    return pagina.evaluate("JSON.parse(JSON.stringify(window.SPIKE.estado))")


def main():
    falhas = []

    def cobrar(condicao, texto):
        print(("  ok   " if condicao else "  FALHOU ") + texto)
        if not condicao:
            falhas.append(texto)

    with tempfile.TemporaryDirectory() as pasta:
        relatorios = pathlib.Path(pasta) / "relatorios.jsonl"
        servidor = subprocess.Popen(
            [str(BINARIO), "--porta", str(PORTA), "--http", str(HTTP), "--relatorios", str(relatorios)],
            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
        )
        try:
            time.sleep(0.8)
            if servidor.poll() is not None:
                print(servidor.stdout.read())
                sys.exit("o servidor não subiu")

            with sync_playwright() as p:
                navegador = p.chromium.launch(args=[
                    "--use-fake-device-for-media-stream",
                    "--use-fake-ui-for-media-stream",
                    "--autoplay-policy=no-user-gesture-required",
                ])

                def nova():
                    contexto = navegador.new_context(permissions=["microphone"])
                    pagina = contexto.new_page()
                    pagina.on("console", lambda m: m.type == "error" and print("    [console]", m.text))
                    pagina.on("pageerror", lambda e: print("    [erro na página]", e))
                    pagina.goto(URL)
                    pagina.wait_for_function("window.SPIKE && window.SPIKE.estado && window.SPIKE.estado.cap")
                    return pagina

                a, b, intrusa = nova(), nova(), nova()

                cap = estado(a)["cap"]
                print("capacidades do Chromium da bancada:",
                      {k: v for k, v in cap.items() if k != "ua"})

                # O caso negativo primeiro: um bit trocado no hash.
                intrusa.evaluate("window.SPIKE.hash[0] ^= 1")
                intrusa.click("#conectar")
                intrusa.wait_for_function("window.SPIKE.estado.conexao.erro || window.SPIKE.estado.conexao.ok", timeout=15000)
                e = estado(intrusa)["conexao"]
                cobrar(not e["ok"] and e["erro"], f"hash adulterado recusado ({e['erro']})")

                for pagina in (a, b):
                    pagina.click("#conectar")
                    pagina.wait_for_function("window.SPIKE.estado.conexao.ok || window.SPIKE.estado.conexao.erro", timeout=15000)
                    pagina.click("#audio")
                    pagina.wait_for_function("window.SPIKE.estado.audio || window.SPIKE.estado.voz.erros.length", timeout=15000)

                for pagina, nome in ((a, "A"), (b, "B")):
                    c = estado(pagina)["conexao"]
                    cobrar(c["ok"], f"{nome} abriu o WebTransport em {c['ms'] and round(c['ms'])} ms "
                                    f"(datagrama máximo {c['maxDatagrama']}, escrita por {c['escrita']})")

                a.check("#eco")
                a.check("#falar")
                time.sleep(FALANDO_S)
                a.uncheck("#falar")
                time.sleep(1.5)
                for pagina in (a, b):
                    pagina.click("#enviar")
                time.sleep(1)

                ea, eb = estado(a), estado(b)
                quadros = FALANDO_S * 50
                va, vb = ea["voz"], eb["voz"]
                print(f"  A: codificados {va['codificados']} · enviados {va['enviados']} · eco n={va['ecoTotal']} · erros {va['erros']}")
                print(f"  B: recebidos {vb['recebidos']} · perdidos {vb['perdidos']} · faltas {vb['faltas']} · "
                      f"descartes {vb['descartes']} · erros {vb['erros']}")
                cobrar(va["enviados"] >= quadros * 0.9, f"A mandou ~{quadros} quadros de 20 ms ({va['enviados']})")
                cobrar(vb["recebidos"] >= va["enviados"] * 0.95, f"B recebeu a voz de A ({vb['recebidos']}/{va['enviados']})")
                cobrar(not va["erros"] and not vb["erros"], "nenhum erro de encoder ou decoder")
                # A fila de B toca de verdade: o buffer de reprodução viu a voz de A.
                cobrar(len(vb["bufferMs"]) >= 1, f"a reprodução de B conhece quem fala ({vb['bufferMs']})")
                # Uma falta por surto de fala é o fim da fala; muitas são a fila esvaziando no meio.
                cobrar(vb["faltas"] <= 3, f"a reprodução de B não ficou sem áudio no meio ({vb['faltas']} faltas)")
                # O contador de som tocado é o que responde, no iPhone, se a voz
                # tocou com a tela bloqueada. Ele tem de contar a fala de A em B
                # (375 blocos de 128 amostras por segundo) e nada em A, que não
                # ouve ninguém — sem o segundo, um contador de silêncio passaria.
                # A metade, e não perto de tudo: o microfone falso do Chromium é
                # um bipe com intervalos, e B tocou som em 69% dos blocos na
                # medida de 07/10/2026. O tom contínuo do `?sintetico` chega perto
                # de 100%, mas não é ele que fala aqui.
                cobrar(vb.get("comSom", 0) >= FALANDO_S * 375 * 0.5,
                       f"B contou som tocado pelo tempo da fala de A ({vb.get('comSom')} blocos, ~{FALANDO_S * 375} esperados)")
                cobrar(va.get("comSom", -1) == 0, f"A, que não ouve ninguém, não contou som ({va.get('comSom')} blocos)")

                eco = sorted(va["eco"])
                rtt = sorted(ea["rtt"]["amostras"])
                cobrar(va["ecoTotal"] >= quadros * 0.9, f"o eco voltou ({va['ecoTotal']} quadros)")
                if eco and rtt:
                    print(f"  ida e volta da voz (captura→opus→rede→decode): "
                          f"p50 {eco[len(eco)//2]:.1f} ms · p95 {eco[int(len(eco)*.95)]:.1f} ms")
                    print(f"  ping por datagrama: p50 {rtt[len(rtt)//2]:.2f} ms · p95 {rtt[int(len(rtt)*.95)]:.2f} ms "
                          f"({ea['rtt']['voltaram']}/{ea['rtt']['enviados']})")

                navegador.close()

            linhas = [json.loads(l) for l in relatorios.read_text().splitlines()] if relatorios.exists() else []
            vias = {l["via"] for l in linhas}
            cobrar(any(l["via"] == "webtransport" for l in linhas), f"relatórios chegaram pelo WebTransport ({len(linhas)} linhas, vias {sorted(vias)})")
            cobrar(any(l["relatorio"].get("motivo") == "falha ao conectar" for l in linhas),
                   "a recusa da intrusa chegou ao servidor por HTTP")
        finally:
            servidor.terminate()
            saida = servidor.communicate(timeout=5)[0]
            if falhas:
                print("\n--- saída do servidor ---\n" + saida)

    print("\nPROVA " + ("PASSOU" if not falhas else f"FALHOU ({len(falhas)})"))
    sys.exit(1 if falhas else 0)


if __name__ == "__main__":
    main()
