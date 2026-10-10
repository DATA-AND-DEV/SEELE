"""O api/v6.json a partir do rascunho (tarefa 12 da F3).

    python3 -I gerar-v6.py <rascunho.json> <saída>

O que sai do rascunho e por quê:
- o processo (o estado, as decisões do dono, as regras do guarda) não é
  contrato;
- os apontadores em prosa e os «novo:» viram o símbolo do código, em chaves
  planas (o check-api só confere o primeiro nível), e a prosa vai para
  `sentido`;
- o que estava EM ABERTO à espera da M3 sai com a medida de 10/10;
- os tetos que o check-api confere são números.
"""

import collections
import json
import sys

O = collections.OrderedDict

r = json.load(open(sys.argv[1]), object_pairs_hook=O)

# Os apontadores, um a um: o símbolo do código, e o que a prosa do rascunho
# dizia, que vai para `sentido`.
READS = [
    ("resposta", "ModReply", None),
    ("contexto.pessoa", "PersonId", None),
    ("contexto.canal", "pedidos::contexto_do_canal", r["reads"]["contexto.canal"]),
    ("contexto.sala", "Occupancy::onde_esta", r["reads"]["contexto.sala"]),
    ("retrato.eu", "projetarORetrato", "quem usa (de Snapshot::me)"),
    ("retrato.servidor.nome", "projetarORetrato", "o nome do servidor (de Snapshot::server)"),
    ("retrato.pessoas", "projetarORetrato", "id, apelido, falando, mudo — dos presentes (de Snapshot::presentes)"),
    ("retrato.salas", "projetarORetrato", "id, nome, limite, pessoas, minha (de Snapshot::voice_rooms)"),
    ("retrato.canais", "projetarORetrato", "id, nome, aberto (de Snapshot::channels)"),
    ("retrato.onde", "cascaDoRetrato", "computador | celular"),
    ("retrato.regioes", "cascaDoRetrato", "as regiões e camadas que existem nesta casca agora"),
    ("capacidades", "capacidades_da_api", None),
    ("servidor.contexto", "pedidos::executar", r["reads"]["servidor"]["contexto"]
     if isinstance(r["reads"].get("servidor"), dict) else r["reads"]["servidor.contexto"]),
]
ACTIONS_NOVOS = {
    "retrato": ("projetarORetrato", "SeeleUI.retrato, no lugar de SeeleMods.snapshot"),
    "formulario-erros": ("SuperficieDeMod::marcarErros", r["actions"]["formulario-erros"].removeprefix("novo: ")),
    "atualizar-contribuicao": ("RegistroDeContribuicoes::atualizar", "atualiza uma contribuição de pé, pelo punho (PERFIS R7)"),
    "superficie-atualizar": ("SuperficieDeMod::atualizar", r["actions"]["superficie-atualizar"].removeprefix("novo na 6: ")),
}
WORLD_NOVOS = {
    "mundo.avisar": ("aviso::aviso_da_chamada", r["world"]["mundo.avisar"]),
    "mundo.tema": ("mundo::valores_do_tema", r["world"]["mundo.tema"]),
}

v6 = O()
v6["version"] = 6
v6["notes"] = r["notes"]
v6["compatibilidade"] = r["compatibilidade"]
v6["handlers"] = r["handlers"]

sentido = O()
reads = O()
for nome, simbolo, prosa in READS:
    reads[nome] = simbolo
    if prosa:
        sentido[nome] = prosa
v6["reads"] = reads
v6["retrato-fica-de-fora"] = r["retrato-fica-de-fora"]

actions = O()
for nome, valor in r["actions"].items():
    if nome in ACTIONS_NOVOS:
        actions[nome], sentido[nome] = ACTIONS_NOVOS[nome]
    else:
        actions[nome] = valor
v6["actions"] = actions
v6["own"] = r["own"]
world = O()
for nome, valor in r["world"].items():
    if nome in WORLD_NOVOS:
        world[nome], sentido[nome] = WORLD_NOVOS[nome]
    else:
        world[nome] = valor
v6["world"] = world
v6["sentido"] = sentido

for bloco in ["moments", "eventos", "eventos-mudam", "eventos-da-tela", "formas", "figuras", "regioes"]:
    v6[bloco] = r[bloco]

contribuicoes = O(r["contribuicoes"])
contribuicoes["teto"] = 128
contribuicoes["alvos-por-ponto"] = 256
contribuicoes["teto-o-que-e"] = (
    "os pontos por alvo (pessoa.*, canal.item, sala.acoes) não contam no teto: uma "
    "contribuição por alvo, por MOD e por modo, até alvos-por-ponto alvos em cada ponto; "
    "o teto é o das contribuições sem alvo de pé, por MOD"
)
v6["contribuicoes"] = contribuicoes

capa = O(r["capa"])
custo = O(capa["custo"])
custo["reducao"] = custo["reducao"].replace(
    "EM ABERTO (M3): proposta de 60 quadros e 10 s", "60 quadros e 10 s, medidos na M3 de 10/10")
custo["animadas"] = custo["animadas"].replace("EM ABERTO (M3): proposta de 6", "6")
capa["custo"] = custo
v6["capa"] = capa

superficies = O(r["superficies"])
folha = O(superficies["folha"])
folha["empilha"] = "3 à vista (superficies.limites.folhas)"
superficies["folha"] = folha
superficies["limites"] = O([
    ("superficies", 12), ("avisos", 4), ("folhas", 3),
    ("nos", 4096), ("fundura", 16), ("campos", 128), ("telas", 4), ("midias", 24),
    ("bytesDeMidia", 24 * 1024 * 1024),
])
superficies["limites-o-que-e"] = (
    "superficies: de pé por MOD; avisos: de pé por MOD; folhas: empilhadas à vista; "
    "nos, fundura, campos, telas, midias e bytesDeMidia: por superfície. A forma midia "
    "entrega até 10 MiB por mídia (§13)"
)
v6["superficies"] = superficies

for bloco in ["camadas", "estilos"]:
    v6[bloco] = r[bloco]

tema = O(r["tema"])
pares = O(tema["pares"])
pares.pop("estado")
tema["pares"] = pares
na7 = list(tema["na-7"])
na7.append("fundo da sessão por mídia (não medido no iPhone antes do congelamento)")
tema.pop("na-6-se-medido")
tema["na-7"] = na7
v6["tema"] = tema

for bloco in ["nucleo-de-confianca", "som", "volume"]:
    v6[bloco] = r[bloco]

limites = O(r["limites"])
limites.pop("estado")
limites.pop("laboratorio")
limites["aviso-do-servidor"] = "1 KiB por aviso e 4 por segundo por MOD"
limites["capas-animadas"] = "6 ao mesmo tempo, contando a do operador, escolhidas pela vista"
limites["quadros-por-capa"] = "60 quadros, 10 s"
limites["folhas"] = "3 à vista"
limites["medido"] = (
    "na M3 de 10/10 (docs/evidencias/m3-custo-dos-mods/medidas-api6.json), com cada ponto no "
    "teto e todos juntos: nenhum commit de quadro acima de um quadro de 60 Hz, o p95 do commit "
    "abaixo de 1 ms, e o jitter da voz no máximo +0,32 ms; cada número vale igual no desktop e "
    "no navegador, porque o motor é o mesmo"
)
v6["limites"] = limites

for bloco in ["celular", "recusas"]:
    v6[bloco] = r[bloco]

# Nada de processo, de «novo:» ou de EM ABERTO no que se publica.
texto = json.dumps(v6, ensure_ascii=False, indent=2) + "\n"
for marca in ["EM ABERTO", "novo:", "RASCUNHO", "proposta de"]:
    assert marca not in texto, marca
open(sys.argv[2], "w").write(texto)
print(f"{sys.argv[2]}: {len(texto)} bytes, {len(v6)} blocos")
