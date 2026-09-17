// O catálogo de MODs, apertado de verdade — ADR 0045.
//
// Prova o que nenhum guarda de texto-fonte alcança: que BUSCAR desenha o que o
// catálogo devolveu, que INSTALAR pede **aquele** MOD naquela versão, e que um
// catálogo cuja assinatura não confere vira uma frase que manda parar — e não
// uma lista vazia, que é como uma recusa de segurança some de vista.

const CATALOGO = {
  esquema: 1,
  gerado_em: 1757100000,
  mods: [
    {
      id: "alguem/rpg",
      titulo: "Salas de RPG",
      resumo: "Fichas, dados e salas por mesa.",
      repo: "https://example.invalid/rpg",
      nivel: "verificado",
      versoes: [
        {
          versao: "1.0.0",
          api: 1,
          hash: "9f2c",
          nivel: "com-notas",
          notas: ["fala-com-terceiro"],
          alcanca: ["falar com um serviço de fora"],
          arquivos: ["mod.json", "cliente/main.js"],
        },
        {
          versao: "1.1.0",
          api: 1,
          hash: "4b71",
          nivel: "verificado",
          notas: [],
          alcanca: [],
          arquivos: ["mod.json", "cliente/main.js"],
        },
      ],
    },
  ],
};

SEELE_RESPOSTAS.catalogo_de_mods = CATALOGO;
SEELE_RESPOSTAS.mods_instalados = [];
SEELE_RESPOSTAS.estou_hospedando = false;

// Abrir CONFIGURAÇÕES e a seção MODS.
document.getElementById("botao-server").click();
await espera(150);
document.getElementById("secao-mods").click();
await espera(200);

relatar(`painel MODS: ${visivel("painel-mods")}`);
// Nada foi buscado ao abrir — ADR 0026.
relatar(`buscou ao abrir: ${window.__SEELE_CHAMADAS.includes("catalogo_de_mods")}`);

document.getElementById("catalogo-buscar").click();
await espera(300);
relatar(`linhas no catálogo: ${document.querySelectorAll("#lista-catalogo li").length}`);
relatar(`estado: ${document.getElementById("catalogo-estado").textContent}`);
// O selo tem de ser o da versão exibida — a 1.1.0, verificada — e não o do MOD.
relatar(
  `selo da versão exibida: ${document.querySelector("#lista-catalogo .mods-versao:last-of-type")?.textContent ?? "—"}`,
);

window.__SEELE_CHAMADAS.length = 0;
document.querySelector("#lista-catalogo button").click();
await espera(300);
const pedido = window.__SEELE_ARGS.instalar_mod_do_catalogo ?? {};
relatar(`pediu instalação: id=${pedido.id} versao=${pedido.versao}`);

// E a recusa de assinatura: tem de virar frase, e não lista vazia.
SEELE_RESPOSTAS.catalogo_de_mods = { __recusa: "AssinaturaDoCatalogoRecusada" };
document.getElementById("catalogo-buscar").click();
await espera(300);
const estado = document.getElementById("catalogo-estado");
relatar(`recusa vira frase: ${estado.textContent.split("\n")[0]}`);
relatar(`marcada como erro: ${estado.classList.contains("mods-recusado")}`);
