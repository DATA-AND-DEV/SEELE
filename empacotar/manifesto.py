#!/usr/bin/env python3
"""Monta o `latest.json` — o manifesto que o botão de atualizar e o launcher leem.

    python3 empacotar/manifesto.py entrega v0.16.0 \\
        [--repo DATA-AND-DEV/SEELE-RELEASES] [--anterior latest-anterior.json]...

Lê o que está em `entrega/`, acha cada arquivo assinado pelo `.sig` ao lado
dele, e escreve o manifesto. Não assina nada: quem assina é o `cargo tauri
build` (ou o `publicar.sh`, para o instalador e o portátil do Windows) com a
chave do projeto no ambiente.

Existe como arquivo, e não dentro do `release.yml`, porque o caminho sem CI é
real — `empacotar/{windows.ps1,macos.sh,linux.sh}` existem para quando a cota de
Actions acaba, e já aconteceu. Um release montado à mão sem este manifesto é um
release que deixa todo mundo sem atualização até o próximo, e a pessoa que o
montou não teria como saber. Uma lógica, um lugar.

O manifesto **não** precisa ser assinado: cada entrada carrega a assinatura do
pacote a que se refere, e é essa que o app confere depois de baixar. Trocar o
manifesto por outro só troca qual pacote é oferecido — e o pacote trocado é
recusado na conferência, que é onde a segurança mora. (A lista de revogação é a
exceção, e por isso vem assinada à parte: `docs/versoes-lado-a-lado.md`, §2.)

**Híbrido, desde a 0.16** (spec `2026-10-08-0.16-design.md`, §1). O topo
continua no formato do `tauri-plugin-updater` — `version`, `notes`, `pub_date`,
`platforms` com `url` e `signature` —, porque é ele que o atualizador da 0.15.1
lê, e ele exige `url` e `signature` em todo alvo do topo. Ao lado vão
`schema: 2` e `versions[]`, que o launcher lê: cada app, a linha de servidor
dele, as APIs de MOD que ele roda e, por pacote, a soma e o executável.

**Acumulado.** `--anterior` é o manifesto que está publicado (o de
`releases/latest`), e o que ele lista continua listado. Sem isso, cada release
apagaria a lista das anteriores, e o launcher esqueceria o que existe.

**O `schema` nunca sobe**: a 0.15.1 recusa um manifesto de schema maior que 2.
Campo novo é acréscimo, e acréscimo não muda o número.
"""

import argparse
import datetime
import hashlib
import json
import pathlib
import re
import sys

SCHEMA = 2

# O executável de cada pacote do launcher, relativo à raiz da instalação.
# O do Mac foi medido no `.app` da 0.15.1: o `CFBundleExecutable` é `seele-app`,
# e não `SEELE` como o contrato dizia antes.
EXECUTAVEL_DO_MAC = "SEELE.app/Contents/MacOS/seele-app"
# O portátil do Windows é a carga do instalador: `SEELE.exe` e `seeled.exe` na
# raiz.
EXECUTAVEL_DO_WINDOWS = "SEELE.exe"

# Decisão do dono de 08/10/2026: além da 1, só a linha do Servidor 0.15 continua
# oferecida.
LINHAS_FORA = {"0.11", "0.12", "0.13", "0.14"}

# Até a 0.15, a versão do app e a do servidor eram uma coisa só.
ULTIMA_LINHA_SEM_NUMERACAO_PROPRIA = 15


class Falha(Exception):
    """Uma recusa com frase, que vira saída 1."""


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


def para_o_atualizador(nome: str) -> list[str]:
    """Os alvos do topo: o que o atualizador baixa para trocar o app instalado.

    No Windows e no Linux o instalador **é** o pacote de atualização: o mesmo
    arquivo que uma pessoa baixaria à mão. O portátil do Windows **não** entra
    aqui — ele não sabe trocar a instalação de Program Files.
    """
    if nome.endswith(".app.tar.gz"):
        return alvos_do_mac(nome)
    if nome.endswith(".exe"):
        return ["windows-x86_64"]
    if nome.endswith(".deb"):
        return ["linux-x86_64"]
    return []


def para_o_launcher(nome: str) -> list[tuple[str, str]]:
    """Os pacotes que se abrem **ao lado** de outro, com o executável de cada um.

    O `.exe` do Windows instala por cima da instalação única, e o `.deb` instala
    no sistema: nenhum dos dois serve ao launcher. O portátil serve.
    """
    if nome.endswith(".app.tar.gz"):
        return [(alvo, EXECUTAVEL_DO_MAC) for alvo in alvos_do_mac(nome)]
    if nome.endswith("-portatil.tar.gz"):
        return [("windows-x86_64", EXECUTAVEL_DO_WINDOWS)]
    return []


def _exigir(padrao: str, texto: str, nome: str, onde: pathlib.Path) -> str:
    achado = re.search(padrao, texto)
    if not achado:
        raise Falha(
            f"não achei {nome} em {onde}. O manifesto precisa dele para dizer o que este app fala."
        )
    return achado.group(1)


def servidor_legado(versao: str) -> str | None:
    """A versão do servidor de um app de antes das duas numerações: 0.15.2 → 0.15."""
    partes = re.split(r"[.-]", versao)
    if len(partes) < 3 or not (partes[0].isdigit() and partes[1].isdigit()):
        return None
    maior, menor = int(partes[0]), int(partes[1])
    if maior == 0 and 1 <= menor <= ULTIMA_LINHA_SEM_NUMERACAO_PROPRIA:
        return f"0.{menor}"
    return None


def ler_unidade(raiz: pathlib.Path, versao: str) -> dict:
    """O que este app fala, lido do código que foi compilado.

    Do código, e não de um argumento: um número digitado à mão no dia do
    release é o número que sai errado. O guarda que confere esta leitura contra
    as constantes compiladas é `a_unidade_lida_do_codigo_e_a_que_o_produto_compila`,
    em `apps/seele-app/tests/manifesto_hibrido.rs`.
    """
    version_rs = raiz / "crates/seele-proto/src/version.rs"
    mods_rs = raiz / "crates/seele-proto/src/mods.rs"
    try:
        texto_version = version_rs.read_text(encoding="utf-8")
        texto_mods = mods_rs.read_text(encoding="utf-8")
    except OSError as erro:
        raise Falha(f"não li o código de {raiz}: {erro}") from erro
    protocolo = int(
        _exigir(r"pub const PROTOCOL_VERSION: u8 = (\d+);", texto_version, "PROTOCOL_VERSION", version_rs)
    )
    api = int(_exigir(r"pub const MOD_API_VERSION: u32 = (\d+);", texto_mods, "MOD_API_VERSION", mods_rs))
    lista = _exigir(r"pub const APIS_ACEITAS: &\[u32\] = &\[([^\]]*)\];", texto_mods, "APIS_ACEITAS", mods_rs)
    apis = sorted(int(n) for n in re.findall(r"\d+", lista))
    achado = re.search(r'pub const VERSAO_DO_SERVIDOR: &str = "(\d+\.\d+)";', texto_version)
    servidor = achado.group(1) if achado else servidor_legado(versao)
    if servidor is None:
        raise Falha(
            f"não achei VERSAO_DO_SERVIDOR em {version_rs}, e {versao} é de depois das duas "
            "numerações. Sem a versão do servidor o launcher não sabe a que linha este app pertence."
        )
    return {"protocol": protocolo, "mod_api": api, "server": servidor, "mod_apis": apis}


def linha_de(servidor: str) -> str | None:
    """A linha de uma versão de servidor: 1.0 e 1.3 → 1; 0.15 → 0.15.

    A mesma regra de `Linha::do_servidor`, em `crates/seele-lancador/src/linha.rs`.
    """
    achado = re.fullmatch(r"(0|[1-9]\d*)\.(0|[1-9]\d*)", servidor or "")
    if not achado:
        return None
    maior, menor = int(achado.group(1)), int(achado.group(2))
    if maior == 0:
        return f"0.{menor}" if menor >= 1 else None
    return str(maior)


def ordem_da_linha(linha: str) -> tuple[int, int]:
    partes = linha.split(".")
    return (int(partes[0]), int(partes[1]) if len(partes) > 1 else 0)


def linha_da_entrada(entrada: dict) -> str | None:
    unidade = entrada.get("unit")
    if not isinstance(unidade, dict):
        return None
    return linha_de(unidade.get("server") or "")


def juntar_anteriores(caminhos: list[pathlib.Path]) -> tuple[list[dict], dict | None, dict | None]:
    """As versões, o topo e a lista de revogação dos manifestos anteriores."""
    versoes: list[dict] = []
    topo = None
    revogacoes = None
    for caminho in caminhos:
        try:
            dados = json.loads(caminho.read_text(encoding="utf-8"))
        except (OSError, ValueError) as erro:
            raise Falha(f"não li o manifesto anterior «{caminho}»: {erro}") from erro
        if dados.get("schema") != SCHEMA or not isinstance(dados.get("versions"), list):
            print(
                f"aviso: «{caminho}» não é um manifesto acumulado (sem schema {SCHEMA}): nada dele "
                "entra. É o caso do latest.json da 0.15.1, e a linha 0.15 entra pela 0.15.2."
            )
            continue
        if topo is None and "version" in dados and "platforms" in dados:
            topo = {chave: dados[chave] for chave in ("version", "notes", "pub_date", "platforms") if chave in dados}
        if revogacoes is None and "revocations" in dados:
            revogacoes = dados["revocations"]
        for entrada in dados["versions"]:
            if any(v.get("version") == entrada.get("version") for v in versoes):
                continue
            versoes.append(entrada)
    return versoes, topo, revogacoes


def montar(
    entrega: pathlib.Path, tag: str, repo: str, raiz: pathlib.Path, anteriores: list[pathlib.Path]
) -> dict | None:
    """O manifesto, ou `None` quando não há uma assinatura sequer."""
    versao = tag[1:] if tag.startswith("v") else tag
    unidade = ler_unidade(raiz, versao)
    linha = linha_de(unidade["server"])
    if linha is None:
        raise Falha(f"a versão do servidor «{unidade['server']}» não diz linha nenhuma.")
    atualizador: dict[str, dict[str, str]] = {}
    launcher: dict[str, dict[str, str]] = {}

    # **A ordem é a escolha, e no Windows ela decide qual instalador atualiza.**
    #
    # Dois `.exe` casavam `windows-x86_64`: o `-setup.exe` do NSIS e o
    # `-instalador.exe`, que é nosso. Quem entrar por último no dicionário vence,
    # e a ordem alfabética punha o NSIS por último — ele ganhava por acidente de
    # nome.
    #
    # Ganhar por acidente de nome custou caro. Quem instala pelo nosso e atualiza
    # pelo NSIS tem dois programas diferentes decidindo onde o SEELE mora e que
    # chave de registro o descreve. Relatado assim: «depois da versão 10.1-1 toda
    # vez que atualizo e fecho o app, ao abrir de novo volta pra essa versão» —
    # com a pasta intacta e **uma só** entrada em Programas e Recursos, que é a
    # nossa. O NSIS nunca completou, e o app nunca soube: o atualizador do Tauri
    # descarta o retorno do `ShellExecuteW` e sai com código zero de qualquer
    # jeito.
    #
    # O nosso vem por último de propósito: a atualização passa pelo mesmo
    # instalador que instalou, que sabe onde o app está pelo registro e respeita
    # a pasta que a pessoa escolheu.
    def preferencia(caminho: pathlib.Path) -> tuple[int, str]:
        return (1 if "-instalador.exe" in caminho.name else 0, caminho.name)

    for assinatura in sorted(entrega.glob("*.sig"), key=preferencia):
        pacote = assinatura.with_suffix("")
        if not pacote.exists():
            print(f"aviso: {assinatura.name} assina um arquivo que não está aqui; ignorado.")
            continue
        alvos_do_atualizador = para_o_atualizador(pacote.name)
        alvos_do_launcher = para_o_launcher(pacote.name)
        if not alvos_do_atualizador and not alvos_do_launcher:
            print(f"aviso: não sei para que sistema serve {pacote.name}; fora do manifesto.")
            continue
        url = f"https://github.com/{repo}/releases/download/{tag}/{pacote.name}"
        sig = assinatura.read_text(encoding="utf-8").strip()
        for alvo in alvos_do_atualizador:
            atualizador[alvo] = {"url": url, "signature": sig}
        if alvos_do_launcher:
            soma = hashlib.sha256(pacote.read_bytes()).hexdigest()
            for alvo, executavel in alvos_do_launcher:
                launcher[alvo] = {"url": url, "signature": sig, "sha256": soma, "executable": executavel}

    if not atualizador and not launcher:
        return None

    # RFC 3339, que é o único formato que o plugin analisa. Um `pub_date`
    # malformado faz o manifesto inteiro ser recusado.
    agora = datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    notas = f"SEELE {versao}. As notas estão em https://github.com/{repo}/releases/tag/{tag}"
    nova = {"version": versao, "pub_date": agora, "notes": notas, "unit": unidade, "platforms": launcher}

    anteriores_versoes, topo_anterior, revogacoes = juntar_anteriores(anteriores)
    todas = [nova] + [v for v in anteriores_versoes if v.get("version") != versao]
    guardadas = []
    for entrada in todas:
        sua_linha = linha_da_entrada(entrada)
        if sua_linha is None:
            print(f"aviso: {entrada.get('version')} não declara linha de servidor; fora do manifesto.")
            continue
        if sua_linha in LINHAS_FORA:
            print(f"{entrada.get('version')} é da linha {sua_linha}, que deixou de ser oferecida; fora do manifesto.")
            continue
        guardadas.append(entrada)
    # **A ordem é o contrato** (`docs/versoes-lado-a-lado.md`, §2): a linha mais
    # nova primeiro, e dentro da linha a ordem de publicação, com a nova à
    # frente. O `sort` do Python é estável, também com `reverse=True`.
    guardadas.sort(key=lambda e: ordem_da_linha(linha_da_entrada(e)), reverse=True)

    mais_nova_linha = max(ordem_da_linha(linha_da_entrada(e)) for e in guardadas)
    if ordem_da_linha(linha) == mais_nova_linha or topo_anterior is None:
        topo = {"version": versao, "notes": notas, "pub_date": agora, "platforms": atualizador}
    else:
        # Um conserto de uma linha mais velha não vira a atualização de quem
        # está na mais nova: o topo continua sendo de quem já estava lá.
        topo = topo_anterior

    manifesto = {**topo, "schema": SCHEMA, "versions": guardadas}
    if revogacoes is not None:
        manifesto["revocations"] = revogacoes
    return manifesto


def main() -> int:
    argumentos = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    argumentos.add_argument("entrega", type=pathlib.Path, help="a pasta com os arquivos do release")
    argumentos.add_argument("tag", help="a tag do release, com o «v» na frente")
    argumentos.add_argument(
        "--repo", default="DATA-AND-DEV/SEELE-RELEASES", help="dono/repositório no GitHub"
    )
    argumentos.add_argument(
        "--anterior",
        type=pathlib.Path,
        action="append",
        default=[],
        help="um manifesto já publicado, para acumular (pode repetir)",
    )
    argumentos.add_argument(
        "--raiz",
        type=pathlib.Path,
        default=pathlib.Path(__file__).resolve().parent.parent,
        help="a raiz do código de onde este release foi compilado",
    )
    argumentos.add_argument("--so-unidade", action="store_true", help="só imprime a unidade lida do código")
    opcoes = argumentos.parse_args()

    try:
        versao = opcoes.tag[1:] if opcoes.tag.startswith("v") else opcoes.tag
        if opcoes.so_unidade:
            print(json.dumps(ler_unidade(opcoes.raiz, versao)))
            return 0
        if not opcoes.entrega.is_dir():
            raise Falha(f"«{opcoes.entrega}» não é uma pasta.")
        manifesto = montar(opcoes.entrega, opcoes.tag, opcoes.repo, opcoes.raiz, opcoes.anterior)
    except Falha as falha:
        print(str(falha), file=sys.stderr)
        return 1

    if manifesto is None:
        # Nenhuma assinatura: este release foi montado sem a chave do projeto.
        # Publicar um manifesto vazio seria pior que nenhum — o app pediria,
        # receberia e não acharia o próprio sistema, e a frase na tela seria
        # «não há pacote para este sistema» em vez de «este SEELE não atualiza
        # sozinho».
        print("sem nenhum .sig em " + str(opcoes.entrega) + ": este release não traz manifesto.")
        return 0

    destino = opcoes.entrega / "latest.json"
    destino.write_text(json.dumps(manifesto, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(destino.read_text(encoding="utf-8"))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
