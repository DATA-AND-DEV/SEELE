// Cada campo que o fio carrega confere o tamanho **em bytes** antes de mandar,
// e diz o teto (pendência 58, item 55; lote C55 da F5).
//
// Os campos contavam caracteres (`maxlength`) e o fio conta bytes. Desde o lote
// P61 o núcleo recusa sem derrubar o enlace um comando com um campo grande
// demais, e o registra no `seele.log`; mas a janela não ficava sabendo, e o
// pedido sumia calado. Aqui cada caminho escreve um texto acentuado que cabe na
// contagem de caracteres e passa na de bytes, e exige três coisas: o comando
// não sai, a frase com o teto aparece onde se escreveu, e um texto que cabe no
// teto ainda sai — sem este último, um campo que nunca manda nada passaria.
//
// Os tetos vêm do duble, que os lê do `seele-proto` (`limites_dos_campos`, em
// `tools/carga-da-casca.py`), e não de um número escrito aqui.
//
// **Cada caso roda por conta própria**: um que reprova escreve REPROVADO e o
// seguinte roda. A reversão de um campo reprova a cena daquele campo, e só ela.

const TETOS = await invoke("limites_dos_campos");
exigir(TETOS && typeof TETOS.apelido === "number",
  "o duble não respondeu `limites_dos_campos`, e nenhum caso abaixo conferiria nada");

/** `n` letras `á`: `n` caracteres e `2n` bytes. */
const acentos = (n) => "á".repeat(n);
/** O menor texto acentuado que passa de `teto` bytes, e o maior que cabe. */
const passa = (teto) => acentos(Math.floor(teto / 2) + 1);
const cabe = (teto) => acentos(Math.floor(teto / 2));
/**
 * Os argumentos de cada vez que a janela mandou `cmd` **neste caso**: o que um
 * caso anterior mandou, reprovado ou não, não conta a favor nem contra.
 */
let comecoDoCaso = 0;
const mandou = (cmd) =>
  window.__SEELE_HISTORICO.slice(comecoDoCaso).filter(([c]) => c === cmd).map(([, a]) => a);
const texto = (id) => document.getElementById(id)?.textContent ?? "";
const caso = async (nome, corpo) => {
  comecoDoCaso = window.__SEELE_HISTORICO.length;
  try {
    await corpo();
    relatar(`${nome}: ok`);
  } catch (erro) {
    if (!(erro && erro.reprovado)) relatar(`REPROVADO: ${nome}: estourou: ${erro}`);
  }
};

// ------------------------------------------------- o apelido gravado (Hello)
//
// Na tela de entrada, o SALVAR do perfil grava nesta máquina
// (`escolher_apelido_local`), e o apelido gravado vai no `Hello` da conexão
// seguinte. Grande demais, o `connect` voltava `Unreachable`, e a pessoa lia
// NADA RESPONDEU NESSE ENDEREÇO com o servidor de pé.
await caso("apelido gravado", async () => {
  const antes = window.__SEELE_APELIDO;
  document.getElementById("boot-perfil").click();
  exigir(await ate(() => visivel("perfil") === "VISIVEL", 900), "o perfil não abriu na entrada");
  const grande = passa(TETOS.apelido);
  document.getElementById("perfil-apelido").value = grande;
  document.getElementById("perfil-salvar").click();
  await ate(() => visivel("perfil-erro") === "VISIVEL", 600);
  exigir(window.__SEELE_APELIDO === antes && !mandou("escolher_apelido_local").some((a) => a?.apelido === grande),
    `um apelido de ${grande.length} letras e ${bytesDe(grande)} bytes foi gravado nesta máquina, e vai no Hello`);
  exigir(visivel("perfil-erro") === "VISIVEL" && texto("perfil-erro").includes(`${TETOS.apelido} BYTES`),
    `a recusa não diz o teto no perfil: «${texto("perfil-erro")}»`);
  exigir(visivel("perfil") === "VISIVEL", "o perfil fechou com o apelido recusado");
  const certo = cabe(TETOS.apelido);
  document.getElementById("perfil-apelido").value = certo;
  document.getElementById("perfil-salvar").click();
  exigir(await ate(() => window.__SEELE_APELIDO === certo, 600),
    `um apelido de ${bytesDe(certo)} bytes, no teto, não foi gravado`);
  await ate(() => visivel("perfil") !== "VISIVEL", 300);
});

// --------------------------------------- o nome do servidor, ao hospedar
//
// `#preparar-nome` vai ao registro local (`criar_servidor`) e, depois de o
// servidor subir e a janela entrar, ao `renomear_server`. Conferido antes de
// tudo isso: um nome que não cabe no fio subia o servidor com o nome recusado
// calado lá dentro.
await caso("nome do servidor ao hospedar", async () => {
  document.getElementById("botao-hospedar").click();
  exigir(await ate(() => visivel("tela-preparar") === "VISIVEL", 900), "PREPARAR ESTE SERVIDOR não abriu");
  const grande = passa(TETOS.nome_do_servidor);
  document.getElementById("preparar-nome").value = grande;
  document.getElementById("preparar-confirmar").click();
  await ate(() => visivel("preparar-erro") === "VISIVEL", 600);
  const subiu = mandou("criar_servidor").length > 0 || mandou("hospedar").length > 0;
  exigir(!subiu && !mandou("renomear_server").some((a) => a?.name === grande),
    `um nome de servidor de ${grande.length} letras e ${bytesDe(grande)} bytes subiu o servidor, e o renomear_server dele o núcleo recusa calado`);
  exigir(visivel("preparar-erro") === "VISIVEL" && texto("preparar-erro").includes(`${TETOS.nome_do_servidor} BYTES`),
    `a recusa não diz o teto em PREPARAR: «${texto("preparar-erro")}»`);
  exigir(visivel("tela-preparar") === "VISIVEL", "a tela de preparar saiu com o nome recusado");
  const certo = cabe(TETOS.nome_do_servidor);
  document.getElementById("preparar-nome").value = certo;
  document.getElementById("preparar-confirmar").click();
  exigir(await ate(() => mandou("renomear_server").some((a) => a?.name === certo), 1500),
    `um nome de servidor de ${bytesDe(certo)} bytes, no teto, não chegou ao servidor que subiu`);
});
const porta = document.getElementById("porta");
if (porta && !porta.hidden) document.getElementById("porta-entendi").click();

// Com sessão, para os casos de dentro do servidor.
if (visivel("tela-sessao") !== "VISIVEL") await entrarNaSessao();

// --------------------------------------------------- o apelido na sessão
await caso("apelido na sessão", async () => {
  document.getElementById("operador-quem").click();
  exigir(await ate(() => visivel("perfil") === "VISIVEL", 900), "o perfil não abriu na sessão");
  const grande = passa(TETOS.apelido);
  document.getElementById("perfil-apelido").value = grande;
  document.getElementById("perfil-salvar").click();
  await ate(() => visivel("perfil-erro") === "VISIVEL", 600);
  exigir(!mandou("escolher_apelido").some((a) => a?.apelido === grande),
    `um apelido de ${bytesDe(grande)} bytes saiu para o servidor, e o núcleo o recusa calado`);
  exigir(visivel("perfil-erro") === "VISIVEL" && texto("perfil-erro").includes(`${TETOS.apelido} BYTES`),
    `a recusa não diz o teto no perfil: «${texto("perfil-erro")}»`);
  const certo = cabe(TETOS.apelido);
  document.getElementById("perfil-apelido").value = certo;
  document.getElementById("perfil-salvar").click();
  exigir(await ate(() => mandou("escolher_apelido").some((a) => a?.apelido === certo), 600),
    `um apelido de ${bytesDe(certo)} bytes, no teto, não saiu`);
  await ate(() => visivel("perfil") !== "VISIVEL", 300);
});

// ----------------------------------------- o nome de um canal ou de uma sala
//
// O diálogo de nomear (`#nomear-valor`) é um só para criar e renomear canal e
// sala: a conferência mora nele, e duas portas dele provam que ela vale para
// as duas.
await caso("nome de canal ou sala", async () => {
  const grande = passa(TETOS.nome_de_canal);
  document.getElementById("nova-linha").click();
  exigir(await ate(() => visivel("nomear") === "VISIVEL", 900), "o NOVO CANAL não abriu");
  document.getElementById("nomear-valor").value = grande;
  document.getElementById("nomear-confirmar").click();
  await ate(() => visivel("nomear-erro") === "VISIVEL", 600);
  exigir(!mandou("criar_linha").some((a) => a?.name === grande),
    `um nome de canal de ${grande.length} letras e ${bytesDe(grande)} bytes saiu, e o núcleo o recusa calado`);
  exigir(visivel("nomear-erro") === "VISIVEL" && texto("nomear-erro").includes(`${TETOS.nome_de_canal} BYTES`),
    `a recusa não diz o teto no diálogo: «${texto("nomear-erro")}»`);
  exigir(visivel("nomear") === "VISIVEL", "o diálogo fechou com o nome recusado");
  const certo = cabe(TETOS.nome_de_canal);
  document.getElementById("nomear-valor").value = certo;
  document.getElementById("nomear-confirmar").click();
  exigir(await ate(() => mandou("criar_linha").some((a) => a?.name === certo), 600),
    `um nome de canal de ${bytesDe(certo)} bytes, no teto, não saiu`);
  await ate(() => visivel("nomear") !== "VISIVEL", 600);

  const sala = document.querySelector("button.voice_room-nome");
  exigir(sala, "nenhuma sala com o nome em botão, para renomear");
  sala.click();
  exigir(await ate(() => visivel("nomear") === "VISIVEL", 900), "o RENOMEAR A SALA não abriu");
  document.getElementById("nomear-valor").value = grande;
  document.getElementById("nomear-confirmar").click();
  await ate(() => visivel("nomear-erro") === "VISIVEL", 600);
  exigir(!mandou("renomear_voice_room").some((a) => a?.name === grande),
    `um nome de sala de ${bytesDe(grande)} bytes saiu ao renomear`);
  document.getElementById("nomear-cancelar").click();
});

// ------------------------------------------------- os lugares da sala nova
//
// Ninguém escreve este número: ele é copiado da primeira sala do servidor
// (`desenharCanais`). Um servidor configurado com uma sala de mais lugares que
// o fio aceita fazia a sala nova sair com eles, e o núcleo a recusava calado.
// A cópia que não cabe vira o teto; a que cabe continua valendo.
await caso("lugares da sala nova", async () => {
  const criar = async (nome) => {
    document.getElementById("nova-voice_room").click();
    exigir(await ate(() => visivel("nomear") === "VISIVEL", 900), "a NOVA SALA DE VOZ não abriu");
    document.getElementById("nomear-valor").value = nome;
    document.getElementById("nomear-confirmar").click();
    await ate(() => mandou("criar_voice_room").some((a) => a?.name === nome), 600);
    await ate(() => visivel("nomear") !== "VISIVEL", 300);
    return mandou("criar_voice_room").find((a) => a?.name === nome)?.limit;
  };
  const original = SEELE_QUADRO.voice_rooms[0].limit;
  try {
    SEELE_QUADRO.voice_rooms[0].limit = TETOS.lugares_da_sala + 50;
    await atualizar();
    const grande = await criar("sala grande");
    exigir(Number.isInteger(grande) && grande >= 1 && grande <= TETOS.lugares_da_sala,
      `copiada de uma sala de ${TETOS.lugares_da_sala + 50} lugares, a sala nova saiu com ${grande}, e o fio aceita de 1 a ${TETOS.lugares_da_sala}`);
    exigir(mandou("registrar_da_janela").some((a) => a?.nivel === "aviso" && a?.oQue?.includes(`${TETOS.lugares_da_sala + 50} lugares`)),
      "o ajuste dos lugares da sala nova não foi ao seele.log");
    SEELE_QUADRO.voice_rooms[0].limit = original;
    await atualizar();
    const copiada = await criar("sala copiada");
    exigir(copiada === original,
      `copiada de uma sala de ${original} lugares, que cabe no fio, a sala nova saiu com ${copiada}`);
  } finally {
    SEELE_QUADRO.voice_rooms[0].limit = original;
  }
});

// ------------------------------------- o nome do servidor, em CONFIGURAÇÕES
//
// `#server-nome-valor` manda ao sair do campo (`change`). A recusa fica no
// `#server-servidor-erro`, e o campo continua com o que se escreveu.
await caso("nome do servidor na configuração", async () => {
  document.getElementById("botao-server-sessao").click();
  exigir(await ate(() => visivel("tela-server") === "VISIVEL", 900), "CONFIGURAÇÕES não abriu");
  document.getElementById("secao-servidor").click();
  exigir(await ate(() => visivel("painel-servidor") === "VISIVEL", 900), "ESTE SERVIDOR não abriu");
  const campo = document.getElementById("server-nome-valor");
  const grande = passa(TETOS.nome_do_servidor);
  campo.focus();
  campo.value = grande;
  campo.dispatchEvent(new Event("change"));
  await ate(() => visivel("server-servidor-erro") === "VISIVEL", 600);
  exigir(!mandou("renomear_server").some((a) => a?.name === grande),
    `um nome de servidor de ${grande.length} letras e ${bytesDe(grande)} bytes saiu, e o núcleo o recusa calado`);
  exigir(visivel("server-servidor-erro") === "VISIVEL" &&
      texto("server-servidor-erro").includes(`${TETOS.nome_do_servidor} BYTES`),
    `a recusa não diz o teto em ESTE SERVIDOR: «${texto("server-servidor-erro")}»`);
  exigir(campo.value === grande, "o campo perdeu o nome recusado, e não há o que encurtar");
  const certo = cabe(TETOS.nome_do_servidor);
  campo.value = certo;
  campo.dispatchEvent(new Event("change"));
  exigir(await ate(() => mandou("renomear_server").some((a) => a?.name === certo), 600),
    `um nome de servidor de ${bytesDe(certo)} bytes, no teto, não saiu`);
  document.getElementById("server-fechar").click();
  await ate(() => visivel("tela-server") !== "VISIVEL", 600);
});

relatar(`tempo virtual gasto: ${Math.round(performance.now())} ms`);
