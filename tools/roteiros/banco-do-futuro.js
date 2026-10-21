// O banco de uma versão mais nova (spec da 0.16, §8). O servidor o recusa sem
// tocar nele (`persistence::BancoDoFuturo`), e hospedar volta com
// `FalhaAoHospedar::BancoDoFuturo`.
//
// **Reprova** se a tela não disser que os dados são de uma versão mais nova e
// que atualizar resolve, ou se cair no NÃO CONSEGUI SUBIR ou no ALGO FALHOU com
// o JSON cru.
SEELE_RESPOSTAS.hospedar = { __recusa: { BancoDoFuturo: { gravado: 17, conhecido: 16 } } };
document.getElementById("botao-hospedar").click();
if (await ate(() => visivel("tela-preparar") === "VISIVEL", 900)) {
  document.getElementById("preparar-confirmar").click();
}
const erro = document.getElementById("boot-erro");
exigir(await ate(() => !erro.hidden && erro.textContent !== "", 1500), "a recusa de hospedar não apareceu");
const dita = erro.textContent;
relatar("hospedar: " + dita.split("\n")[0]);
exigir(dita.startsWith("OS DADOS DESTE SERVIDOR SÃO DE UMA VERSÃO MAIS NOVA DO SEELE."),
  "a tela não disse que os dados são de uma versão mais nova: " + dita);
exigir(/atualize o app/i.test(dita), "a tela não disse o que fazer: " + dita);
exigir(dita.includes("esquema 17") && dita.includes("até o 16"), "os números para quem relata sumiram: " + dita);
