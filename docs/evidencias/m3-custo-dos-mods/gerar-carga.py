#!/usr/bin/env python3
"""M3 da 0.16 — gera as variantes do MOD de carga sintético da API 6.

    python3 -I gerar-carga.py <pasta-de-pacotes>

Escreve `<pasta>/m3-carga-<cenario>/` para cada cenário, a partir do modelo
`m3-carga/` ao lado deste arquivo: o cenário vai no `mod.json` (a versão) e na
metade de cliente (`CENARIO`), e os dois GIFs da capa, em base64, na metade de
servidor. Cada variante tem o seu hash, e a preparação a põe no cache da
medição: o MOD de carga não está no indexador.

**Os GIFs, no teto do bolso das capas** (`LIMITES_DA_CAPA`, 128 MiB
decodificados por MOD; `TETO_DA_CAPA`, 960 × 320, 60 quadros, 10 s): sete
animadas de 480 × 144 com 60 quadros de 160 ms (9,6 s), 16,6 MB cada
decodificadas, e vinte paradas do mesmo tamanho, 0,28 MB cada — 121,6 MB, 91 %
do bolso. No tamanho máximo, uma animada sozinha seria 73,7 MB, e a segunda já
seria recusada: o cenário mediria a recusa, e não o custo.

O codificador é o mínimo que serve, sem dependência: LZW só com literais e um
`clear` a cada 126 códigos (o código fica em 8 bits), e cada quadro depois do
primeiro redesenha só a faixa que andou. Os navegadores e o `image` do Rust o
leem como qualquer GIF.
"""

import base64
import pathlib
import struct
import sys

AQUI = pathlib.Path(__file__).resolve().parent
MODELO = AQUI / "m3-carga"
CENARIOS = ["regioes", "capas", "folhas", "avisos", "dialogo", "pagina", "palco", "tudo"]

LARGURA, ALTURA = 480, 144
QUADROS, ATRASO_CS = 60, 16  # 60 × 160 ms = 9,6 s, abaixo dos 10 s do teto
FAIXA = 48                    # a largura da faixa que anda

# 128 cores (o código mínimo de 7 bits); só as quatro primeiras são usadas.
PALETA = [(0x12, 0x10, 0x0E), (0xC8, 0x6B, 0x1F), (0xEA, 0xE3, 0xCF), (0x24, 0x1F, 0x19)] + [(0, 0, 0)] * 124


def lzw_literal(indices):
    """Os índices como códigos literais de 8 bits, com um `clear` a cada 126:
    o decodificador soma uma entrada por código, e o `clear` volta antes de a
    tabela chegar a 256, quando o código passaria a 9 bits."""
    clear, eoi = 128, 129
    codigos = []
    for i, k in enumerate(indices):
        if i % 126 == 0:
            codigos.append(clear)
        codigos.append(k)
    codigos.append(eoi)
    # 8 bits por código, do menos significativo para o mais: um byte cada.
    dados = bytes(codigos)
    blocos = bytearray([7])  # o tamanho mínimo do código
    for i in range(0, len(dados), 255):
        pedaco = dados[i:i + 255]
        blocos += bytes([len(pedaco)]) + pedaco
    blocos += b"\x00"
    return bytes(blocos)


def quadro(x, y, w, h, indices, atraso):
    controle = b"\x21\xf9\x04" + bytes([0x04]) + struct.pack("<H", atraso) + b"\x00\x00"
    descritor = b"\x2c" + struct.pack("<HHHH", x, y, w, h) + b"\x00"
    return controle + descritor + lzw_literal(indices)


def gif(animado):
    cabeca = b"GIF89a" + struct.pack("<HH", LARGURA, ALTURA) + bytes([0xF6, 0, 0])
    cabeca += b"".join(bytes(c) for c in PALETA)
    if animado:
        cabeca += b"\x21\xff\x0bNETSCAPE2.0\x03\x01\x00\x00\x00"
    # O primeiro quadro, inteiro: o fundo e uma borda clara.
    fundo = []
    for y in range(ALTURA):
        for x in range(LARGURA):
            borda = x < 2 or y < 2 or x >= LARGURA - 2 or y >= ALTURA - 2
            fundo.append(2 if borda else 0)
    corpo = quadro(0, 0, LARGURA, ALTURA, fundo, ATRASO_CS if animado else 0)
    if animado:
        # Os outros 59: a faixa laranja anda; a que ficou para trás volta ao fundo.
        passo = (LARGURA - FAIXA) // (QUADROS - 1)
        for n in range(1, QUADROS):
            x = n * passo
            de = max(0, x - passo)
            w = min(LARGURA - de, x + FAIXA - de)
            indices = []
            for yy in range(ALTURA):
                for xx in range(de, de + w):
                    indices.append(1 if x <= xx < x + FAIXA and 4 <= yy < ALTURA - 4 else 0)
            corpo += quadro(de, 0, w, ALTURA, indices, ATRASO_CS)
    return cabeca + corpo + b"\x3b"


def main():
    if len(sys.argv) != 2:
        sys.exit("uso: gerar-carga.py <pasta-de-pacotes>")
    saida = pathlib.Path(sys.argv[1])
    animado = base64.b64encode(gif(True)).decode()
    parado = base64.b64encode(gif(False)).decode()
    for cenario in CENARIOS:
        destino = saida / f"m3-carga-{cenario}"
        for origem in MODELO.rglob("*"):
            if origem.is_dir():
                continue
            relativo = origem.relative_to(MODELO)
            texto = origem.read_text()
            texto = texto.replace("__CENARIO__", cenario)
            texto = texto.replace("__GIF_ANIMADO__", animado).replace("__GIF_PARADO__", parado)
            alvo = destino / relativo
            alvo.parent.mkdir(parents=True, exist_ok=True)
            alvo.write_text(texto)
        print(f"m3-carga-{cenario}: {destino}")
    print(f"gif animado: {len(animado)} caracteres em base64; parado: {len(parado)}")


if __name__ == "__main__":
    main()
