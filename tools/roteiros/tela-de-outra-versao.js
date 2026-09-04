// A transmissão que esta versão não sabe ler.
//
// Relatado assim: «quem assiste com uma versão mais velha vê tela preta, sem
// mensagem nenhuma, e a sessão morre em ~3 segundos sem dizer por quê». A parte
// «sem mensagem nenhuma» era o núcleo voltando calado: sem `ScreenOpened` não
// havia o que desenhar, sem `ScreenClosed` não havia o que apagar.
//
// Aqui o evento é entregue à mão, porque produzi-lo de verdade pediria dois
// builds de versões diferentes. O que se mede é o que a casca faz com ele.

document.getElementById("botao-hospedar").click();
await espera(900);
const porta = document.getElementById("porta");
if (porta && !porta.hidden) { document.getElementById("porta-entendi").click(); await espera(200); }

function palco() {
  const onde = $("palco-falha");
  return onde.hidden ? "sem frase" : onde.textContent;
}

relatar("antes: " + palco());

// O que a ponte manda quando o cabeçalho não decodifica. Os ouvintes ficam
// guardados pela carga, e é por eles que o evento entra.
for (const ouvinte of window.__SEELE_OUVINTES["seele://event"] ?? []) {
  ouvinte({
    payload: { ScreenUnreadable: { reason: "unsupported version 3, expected 2" } },
  });
}
await espera(300);
relatar("depois do evento: " + palco());
