// Quem cola um código em CONECTAR liga, e a tela segue o que o Rust leu
// (tarefa 30 da F7; spec de 23/09, «Como um texto colado é classificado» e «Do
// lado de quem liga», 1). A metade de dentro de uma sessão é
// `ligar-na-sessao.js`.
//
// O duble não tem a leitura do código nem a chave desta máquina: cada passo põe
// em `SEELE_RESPOSTAS.ler_entrada` a resposta que o Rust daria
// (`ligar::EntradaColada`), e confere o que a tela fez com ela.
//
// 1. **O erro de digitação é dito sem consultar a rede**, e nada conecta.
// 2. **O próprio código também**: «ESSE É O SEU CÓDIGO».
// 3. **O código de outra pessoa vira o alvo `@CÓDIGO`**, que vai ao `connect`.

const CODIGO = "CSM7-NBFR-CAYQ-EV4Y";
const ALVO = "@" + CODIGO;

const chamou = (cmd) => window.__SEELE_CHAMADAS.includes(cmd);
const erroDoDialogo = () =>
  visivel("servidores-erro") === "VISIVEL"
    ? document.getElementById("servidores-erro").textContent
    : "";
const colar = async (texto, pronto) => {
  await abrirServidores();
  document.getElementById("servidores-endereco").value = texto;
  window.__SEELE_CHAMADAS.length = 0;
  document.getElementById("servidores-forma").dispatchEvent(
    new Event("submit", { cancelable: true }),
  );
  await ate(pronto, 600);
};

// 1. Erro de digitação.
SEELE_RESPOSTAS.ler_entrada = "CodigoComErro";
await colar("CSM7-NBFR-CAYQ-EV4Z", () => erroDoDialogo() !== "");
relatar(`erro de digitação: «${erroDoDialogo()}» · conectou ${chamou("connect")}`);
exigir(chamou("ler_entrada"), "o texto colado não foi ao `ler_entrada`");
exigir(erroDoDialogo() === ENTRADA_COLADA.CodigoComErro,
  `o código com erro de digitação não foi dito: «${erroDoDialogo()}»`);
exigir(!chamou("connect"), "um código com erro de digitação conectou");
fecharServidores();

// 2. O próprio código.
SEELE_RESPOSTAS.ler_entrada = "SeuCodigo";
await colar(CODIGO, () => erroDoDialogo() !== "");
relatar(`o próprio código: «${erroDoDialogo()}» · conectou ${chamou("connect")}`);
exigir(erroDoDialogo() === ENTRADA_COLADA.SeuCodigo,
  `o próprio código não foi dito: «${erroDoDialogo()}»`);
exigir(!chamou("connect"), "colar o próprio código conectou");
fecharServidores();

// 3. O código de outra pessoa, da entrada: liga para o alvo canônico.
SEELE_RESPOSTAS.ler_entrada = { Codigo: { codigo: CODIGO, alvo: ALVO } };
delete window.__SEELE_ARGS.connect;
await colar(CODIGO, () => chamou("connect"));
const pedido = window.__SEELE_ARGS.connect ?? {};
relatar(`ligou para: ${pedido.server}`);
exigir(pedido.server === ALVO, `o código não virou o alvo de quem liga: ${pedido.server}`);
exigir(visivel("servidores") !== "VISIVEL", "o diálogo ficou aberto depois de ligar");
