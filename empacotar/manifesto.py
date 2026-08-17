#!/usr/bin/env python3
"""Monta o `latest.json` — o manifesto que o botão de atualizar procura.

    python3 empacotar/manifesto.py entrega v1.2.3

Lê o que está em `entrega/`, acha cada arquivo assinado pelo `.sig` ao lado
dele, e escreve o manifesto que o `tauri-plugin-updater` sabe ler. Não assina
nada: quem assina é o `cargo tauri build` com a chave do projeto no ambiente.

Existe como arquivo, e não dentro do `release.yml`, porque o caminho sem CI é
real — `empacotar/{windows.ps1,macos.sh,linux.sh}` existem para quando a cota de
Actions acaba, e já aconteceu. Um release montado à mão sem este manifesto é um
release que deixa todo mundo sem atualização até o próximo, e a pessoa que o
montou não teria como saber. Uma lógica, um lugar.

O manifesto **não** precisa ser assinado: cada entrada carrega a assinatura do
pacote a que se refere, e é essa que o app confere depois de baixar. Trocar o
manifesto por outro só troca qual pacote é oferecido — e o pacote trocado é
recusado na conferência, que é onde a segurança mora.
"""

import argparse
import datetime
import json
import pathlib
import sys

# Cada `.sig` aponta para o arquivo que ele assina, e a extensão desse arquivo
# diz para quem ele serve. Descoberto assim, e não por uma lista de nomes: nome
# de instalador muda com o empacotador, e uma lista escrita à mão envelhece
# calada.
#
# Os nomes dos alvos são os que o plugin monta em tempo de execução — sistema e
# arquitetura, `darwin` e não `macos`. Errar um deles produz um manifesto que
# carrega e no qual o app não se encontra.
POR_SUFIXO = {
    # O `.app` compactado, e não o `.dmg`: imagem de disco é coisa que uma
    # pessoa monta e arrasta. Quais Macs ele serve é o `alvos_do_mac` que diz.
    ".app.tar.gz": None,
    # No Windows e no Linux o instalador **é** o pacote de atualização: o mesmo
    # arquivo que uma pessoa baixaria à mão.
    ".exe": ["windows-x86_64"],
    ".deb": ["linux-x86_64"],
}


def alvos_do_mac(nome: str) -> list[str]:
    """Quais Macs este pacote serve, lido do nome do arquivo.

    O workflow produz um binário universal e o chama de `_universal`; ele serve
    os dois, e cada máquina pergunta pelo seu. Mas `empacotar/macos.sh` compila
    **só a arquitetura da máquina onde roda** — está escrito no cabeçalho dele —
    e um release montado assim, oferecido aos dois, entregaria a um Mac Intel um
    app que não abre. A conferência não é hipotética: foi a falta de CI que
    obrigou a empacotar à mão, e é nesse dia que este arquivo é escrito.
    """
    if "aarch64" in nome or "arm64" in nome:
        return ["darwin-aarch64"]
    if "x86_64" in nome or "intel" in nome:
        return ["darwin-x86_64"]
    return ["darwin-aarch64", "darwin-x86_64"]


def montar(entrega: pathlib.Path, tag: str, repo: str) -> dict | None:
    """O manifesto, ou `None` quando não há uma assinatura sequer."""
    versao = tag[1:] if tag.startswith("v") else tag
    plataformas: dict[str, dict[str, str]] = {}

    for assinatura in sorted(entrega.glob("*.sig")):
        pacote = assinatura.with_suffix("")
        if not pacote.exists():
            print(f"aviso: {assinatura.name} assina um arquivo que não está aqui; ignorado.")
            continue
        for sufixo, alvos in POR_SUFIXO.items():
            if pacote.name.endswith(sufixo):
                url = f"https://github.com/{repo}/releases/download/{tag}/{pacote.name}"
                for alvo in alvos or alvos_do_mac(pacote.name):
                    plataformas[alvo] = {
                        "url": url,
                        "signature": assinatura.read_text().strip(),
                    }
                break
        else:
            print(f"aviso: não sei para que sistema serve {pacote.name}; fora do manifesto.")

    if not plataformas:
        return None

    return {
        "version": versao,
        "notes": f"SEELE {versao}. As notas estão em https://github.com/{repo}/releases/tag/{tag}",
        # RFC 3339, que é o único formato que o plugin analisa. Um `pub_date`
        # malformado faz o manifesto inteiro ser recusado.
        "pub_date": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "platforms": plataformas,
    }


def main() -> int:
    argumentos = argparse.ArgumentParser(description=__doc__)
    argumentos.add_argument("entrega", type=pathlib.Path, help="a pasta com os arquivos do release")
    argumentos.add_argument("tag", help="a tag do release, com o «v» na frente")
    argumentos.add_argument("--repo", default="DATA-AND-DEV/SEELE", help="dono/repositório no GitHub")
    opcoes = argumentos.parse_args()

    if not opcoes.entrega.is_dir():
        print(f"«{opcoes.entrega}» não é uma pasta.", file=sys.stderr)
        return 1

    manifesto = montar(opcoes.entrega, opcoes.tag, opcoes.repo)
    if manifesto is None:
        # Nenhuma assinatura: este release foi montado sem a chave do projeto.
        # Publicar um manifesto vazio seria pior que nenhum — o app pediria,
        # receberia e não acharia o próprio sistema, e a frase na tela seria
        # «não há pacote para este sistema» em vez de «este SEELE não atualiza
        # sozinho».
        print("sem nenhum .sig em " + str(opcoes.entrega) + ": este release não traz manifesto.")
        return 0

    destino = opcoes.entrega / "latest.json"
    destino.write_text(json.dumps(manifesto, indent=2, ensure_ascii=False) + "\n")
    print(destino.read_text())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
