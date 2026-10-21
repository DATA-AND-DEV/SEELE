// O APAGAR de um canal quando o servidor recusa a pesagem (`WeighChannel`,
// pelo `nao_deu`). A FFI devolve a recusa a quem perguntou
// (`ConnectionError::PedidoRecusado`), e a caixa abre dizendo que não deu para
// contar, por quê e o que fazer.
//
// Era um APAGAR sem resposta nenhuma: a espera do peso ficava viva até a sessão
// acabar, e a caixa nunca abria. **Reprova** se a caixa não abrir, se não disser
// o porquê e o que fazer, ou se cair no ALGO FALHOU com o JSON cru.
SEELE_RESPOSTAS.peso_da_linha = { __recusa: { PedidoRecusado: { causa: "FalhaDoServidor" } } };
await entrarNaSessao();
const apagar = document.querySelector("#lista-linhas button[data-apagar-linha]");
exigir(apagar !== null, "não há botão de apagar canal na lista de canais");
apagar.click();
const caixa = document.getElementById("moderar");
exigir(await ate(() => !caixa.hidden, 1500), "a caixa do APAGAR não abriu depois da recusa da pesagem");
const dita = document.getElementById("moderar-consequencia").textContent;
relatar("caixa: " + dita.split("\n")[0]);
exigir(dita.startsWith("Não consegui contar o que há dentro de #"), "a caixa não diz que não deu para contar: " + dita);
exigir(dita.includes("O SERVIDOR NÃO CONSEGUIU."), "a caixa não diz por quê: " + dita);
exigir(dita.includes("quem hospeda acha o motivo no registro do servidor"), "a caixa não diz o que fazer: " + dita);
exigir(!dita.includes("ALGO FALHOU"), "a recusa caiu no ALGO FALHOU: " + dita);
