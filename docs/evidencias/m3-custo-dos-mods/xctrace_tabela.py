"""Lê uma tabela exportada pelo `xctrace export --xpath`, resolvendo `id`/`ref`.

O xctrace escreve um valor repetido uma vez só, com `id="N"`, e as linhas
seguintes o citam por `ref="N"`. Ler o texto da linha sem resolver isso conta
só a primeira ocorrência de cada processo.
"""
import xml.etree.ElementTree as ET


def linhas(xml_texto: str):
    raiz = ET.fromstring(xml_texto)
    schema = raiz.find(".//schema")
    if schema is None:
        return []
    colunas = [c.findtext("mnemonic") for c in schema.findall("col")]
    vistos = {}

    def valor(el):
        ref = el.get("ref")
        if ref is not None:
            return vistos.get(ref, (None, None))
        par = (el.text, el.get("fmt"))
        # o valor de um elemento composto (processo, narrativa) é o fmt dele
        if el.get("id") is not None:
            vistos[el.get("id")] = par
        for filho in el.iter():
            if filho is not el and filho.get("id") is not None:
                vistos[filho.get("id")] = (filho.text, filho.get("fmt"))
        return par

    saida = []
    for row in raiz.iter("row"):
        celulas = list(row)
        registro = {}
        for nome, el in zip(colunas, celulas):
            registro[nome] = valor(el)
        saida.append(registro)
    return saida
