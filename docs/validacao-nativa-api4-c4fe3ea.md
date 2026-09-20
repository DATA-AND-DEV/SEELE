# Reteste nativo da API 4 — candidata c4fe3ea

Rodada de 20/09/2026. Este registro complementa
[o teste de d96d71a](validacao-nativa-api4-d96d71a.md).

## Ambiente

- Produto `c4fe3ea`; MESA `4effc0c`, PERFIS `a9c21d0`, ESTILO `f46ab63`.
- Build release local, cópia isolada em `target/qa-c4fe3ea/SEELE QA.app`.
- Bundle `tech.datadev.seele.qa.c4fe3ea`, dados em
  `/tmp/SEELE-QA-c4fe3ea/home`. Instalação habitual preservada.
- Pacotes gerados pelos três `package.mjs`, instalados pelo seletor nativo,
  ligados juntos e confirmados pela interface. Um administrador, servidor novo.
- Janela de aproximadamente 1229 × 768, macOS/WKWebView.

## Resultado observado até a interrupção da captura

| Ponto | Evidência nesta rodada |
| --- | --- |
| N1: editor ESTILO | **Abriu**, com seletores e abas. A aba Conjuntos também abriu e apresentou os três presets |
| N2: faixa fixa | **Ausente**, com os três MODs ativos. As entradas ficaram na navegação e a conversa recuperou a altura |
| N3: perfil vazio | **Nome nativo preservado**. Existe botão acessível para abrir o PERFIS, mas sem texto/ícone visível |
| N4: preferência nativa | Ainda não repetida no app nesta rodada |
| N5: criação de campanha | Ainda não repetida no app nesta rodada |
| N6: compositor | **Corrigido** visualmente: campo escuro e largo. Shift+Enter criou segunda linha; Enviar publicou as duas linhas no canal de QA |
| Configurações | Rótulos mais curtos observados. Ainda falta repetir troca de seção/rolagem visual |
| Escape e modais | Ainda não repetidos nesta rodada |

A seleção de **Profundo → Usar este** foi tentada, mas a captura seguinte
falhou no ScreenCaptureKit. Não há confirmação de prévia, publicação ou
restauração a partir desse clique. Não registrar essas jornadas como aprovadas.

## N1: diagnóstico de mensagem grande continua errado com UTF-8

A redução da montagem inicial do ESTILO foi confirmada separadamente:
**4.853 bytes**, em vez de 14.164. O instrumento anterior foi executado em
memória com sua condição de prontidão trocada de `regiao` para `contribuir`,
pois justamente N2 removeu a região. O arquivo histórico não foi alterado.

O novo cálculo em `apps/seele-app/src/executor.rs:904`, porém, usa
`new TextEncoder()` e, se não existir, retorna `texto.length`. No QuickJS
usado pelo produto, `TextEncoder` não existe. `length` mede unidades UTF-16,
não bytes UTF-8. Assim, conteúdo acentuado pode ultrapassar 12 KiB, passar
pela conferência do prelúdio e receber novamente `fila-cheia` do caminho Rust.

Reprodução executada com `rquickjs = 0.12.2`, `Context::full`, prelúdio lido
diretamente de `executor.rs` e ponte que aplica o mesmo teto individual:

```text
TextEncoder no QuickJS: undefined
postar recebeu 14066 bytes
Diagnóstico: fila-cheia
```

Pedido: `SeeleUI.regiao([{forma:'texto', dentro:'é'.repeat(7000)}])`.
Instrumento local preservado em `/tmp/SEELE-QA-c4fe3ea/utf8`, executado por
`cargo run --offline --manifest-path /tmp/SEELE-QA-c4fe3ea/utf8/Cargo.toml`.
É medição do prelúdio no motor real, sem interface, não uma medição de uso
de memória nem da fila completa do aplicativo.

**Correção delimitada:** contar UTF-8 sem depender de API de navegador,
incluindo pares substitutos, ou fornecer a contagem pelo anfitrião. Conservar
o teto Rust. Aceite: texto ASCII, acentos e emoji produzem tamanho correto e
`mensagem-grande`, distinguível de uma fila realmente saturada. A contenção
continua funcionando; o defeito é no diagnóstico.

## Acabamentos observados

- O perfil sem conteúdo ganhou identidade, mas `aPortaDoProvedor` monta um
  botão vazio: `aria-label` ajuda tecnologia assistiva e não cria um rótulo
  visual. Dar texto ou ícone com propósito identificável ao caminho do editor.
- No ESTILO, amostras e controles que pedem `direcao: 'linha'` continuam
  empilhados. `caixa` não tem `display:flex` em `mods-controles.css`, enquanto
  `direcao` só aplica `flex-direction` em `mods-estilos.js`. Usar a primitiva
  `pilha` para os conjuntos flexíveis, ou explicitar esse contrato. A página
  funciona, mas a composição desperdiça altura.
- A linha do operador passou a dizer `microfone abre na tecla · ouvindo`.
  O cabeçalho de configurações ainda diz `microfone aberto`, pois
  `desenharContextoDaSessao` em `tela-server.js:543` só considera `muted`.
  Aplicar ali a mesma distinção entre modo, dispositivo e transmissão.

## Limitação operacional desta rodada

A primeira hospedagem ficou na entrada com `DENTRO`; o log confirmou sessão
aberta e depois conexão perdida. A janela também ficou inacessível à
automação. Após reiniciar a cópia QA e o usuário trazê-la à área de trabalho,
a sessão abriu normalmente. Causa não determinada; não atribuir ao conserto
dos MODs nem apresentar como corrigida.

Na segunda interrupção, após abrir os conjuntos do ESTILO, todas as leituras
da janela passaram a devolver erro do **ScreenCaptureKit -3812**. Reobter o
aplicativo e reiniciar a sessão de automação não recuperou a captura. Foi
solicitado ao usuário conferir a janela. Essa falha da ferramenta limita a
homologação; não é prova de falha do tema. Os passos sem observação permanecem
pendentes.

Build release e bancada `contribuicoes-e-camadas.cjs` passaram nesta rodada.
Não houve alteração no código do produto, commit, push ou publicação.

## Critério visual reforçado pelo usuário

A aparência atual foi rejeitada pelo usuário: os MODs oficiais precisam ter
qualidade e coerência com o SEELE, incluindo os modais de PERFIS e ESTILO.
Classificar a composição atual apenas como pequenos acabamentos minimiza o
problema. Os êxitos funcionais acima não constituem aprovação visual.

A comparação com PERFIS `ce976fd` e ESTILO `0fba9a5` mostra que ambos usavam
`<dialog>` com `showModal()`, CSS próprio, grade de duas colunas, espaçamentos
definidos, controles de largura completa e ações diferenciadas. O código do
MOD criava esses elementos diretamente no documento. Na candidata atual, o
produto monta outra casca em `mods-superficies.js`; PERFIS pede um diálogo e
ESTILO pede uma página. Essa escolha de página não é exigência de QuickJS ou
do isolamento por servidor.

Há também uma perda concreta na transposição do layout: o editor PERFIS
declara `caixa` com `direcao: 'linha'` para campos e prévia lado a lado. A
forma `caixa` recebe `min-width`, sem `display:flex`; `direcao` aplica somente
`flex-direction`. Portanto a intenção declarada não resulta na composição
esperada. No ESTILO acontece o mesmo com várias linhas. Compartilhar cores
do SEELE sem reproduzir sua tipografia, hierarquia, espaçamento e composição
não produz integração visual.

Direção para a próxima implementação:

1. Usar os modais anteriores como referência visual e de interação para
   PERFIS e ESTILO; manter MESA como espaço de jogo, com diálogos para tarefas.
2. Oferecer pelo anfitrião uma casca de modal coerente com os componentes do
   SEELE: cabeçalho, tipografia, bordas, espaçamento, campos, foco e ações.
   Reutilizar estilos/componentes comuns e evitar uma segunda linguagem visual
   exclusiva dos MODs. O MOD continua livre para compor e personalizar o corpo.
3. Abrir Aparência do servidor num modal amplo, com edição e prévia bem
   distribuídas. PERFIS deve ter banner/avatar e prévia destacados; campos
   e prévia lado a lado quando houver espaço e empilhados apenas quando necessário.
4. Corrigir o contrato de layout ou seu uso nos três pacotes. Não compensar
   uma linha que não funciona acrescentando rolagem a um formulário vertical.
5. Comparar visualmente os três MODs com o app e com suas versões anteriores,
   em janela larga e estreita, antes de declarar paridade ou acabamento.

Preservar o mecanismo antigo de acesso direto ao documento não é necessário
para recuperar sua aparência. O anfitrião pode criar o diálogo e seus estilos
na WebView existente, associando todos os recursos à instância do MOD para
descarte ao sair. A fronteira de execução permanece no QuickJS. A discussão
de `<dialog>` versus uma camada controlada pelo anfitrião é de implementação;
nenhuma das duas justifica uma regressão visual.
