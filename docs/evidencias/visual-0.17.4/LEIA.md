# Interface 0.17.4 — bancada visual

As capturas `inicio`, `perfil`, `audio`, `sessao`, `chamada` e `pessoal` mostram o HTML, CSS e JavaScript implementados. Identidade nativa, pessoas e medidas de sessão são fixtures explícitas da bancada, sem conexão com pares nem áudio físico. Não representam medições de desempenho.

`visual-0174.cjs` verificou seis telas em 320, 390, 768, 1280 e 1920 px, com redução de movimento: 30 cenas, nenhum erro JavaScript e nenhum transbordamento horizontal da raiz. `medidas.json` registra também elementos internos que podem rolar (campos de código e trilha horizontal); esse diagnóstico é separado do transbordamento da tela.

Reproduzir com `PLAYWRIGHT=/caminho/node_modules/playwright node apps/seele-app/bancada/visual-0174.cjs /tmp/capturas`. Usa Chromium; a variável seleciona uma instalação existente, sem instalar dependências.

`web-celular-entrada.png` usa a ponte web real, IndexedDB e o WASM compilado, sem identidade simulada. O código foi calculado localmente. O recebimento informa falha real: o endpoint público `/chamada/1` recusou CORS para a origem local nesta rodada. Esta captura não homologa chamada web ↔ desktop. As provas de transporte e persistência ficam nas bancadas próprias dessa funcionalidade.

A bancada `pessoal-web.cjs` passou em Chromium e WebKit (1280/390/320; convite, aceite, texto multilinha, deduplicação, saída e controles). `tons-do-tema.cjs` e `tema-sai-com-a-sessao.cjs` passaram. A segunda recebeu os globais que seu VM não fornecia: o original do HEAD também falhava por `queueMicrotask` ausente; a correção mantém a microtarefa real e simula apenas a carga de MOD, fora do escopo do teste de descarte do tema.

## Complemento de diálogos e entrada

`visual-0174.cjs <pasta> --extras` reaproveita as cenas reais da navegação e acrescenta o editor de cargos. A rodada tem 24 cenas: 1280 e 390 px para autenticação, espera por aprovação, porta, pedidos da portaria, moderação, ajuda, reconexão, fim, entrada celular e aceite de MODs. Verifica transbordamento horizontal e, para cada controle habilitado, rolagem até o alvo, foco e `elementFromPoint` no centro. As capturas ficam em `extras/`; `run.log` registra a rodada completa aprovada. A revisão final do título do aceite repetiu quatro cenas (aceite e cargos); `extras/relatorio.json` contém essa revisão parcial aprovada.

As novas composições mantêm o conteúdo de segurança e os estados do produto. Cabeçalhos e ações ficam separados da rolagem, a portaria usa uma só rolagem para não cortar Admitir/Recusar, cargos organiza regras e editor sem esconder a hierarquia, e a reconexão mantém a contagem real sem piscar. As duas alterações em HTML são os títulos da autenticação, que deixam de apresentar as coordenadas decorativas do comp antigo.

`mods-nav-0174.cjs <pasta>` passou em 1280/390/320: três contribuições pela API 7, ciclos Mesa → Conversa → Mesa, DOM único ao reabrir e teclado (setas/Home/End/Enter). Usa `InstanciaDeMod`, registro, roteador e renderer do produto; a resposta do executor é fixture, sem executar um pacote externo. `mods-mesa-*.png` identifica a demonstração.

`vista-pessoal-0-17.cjs` passou em Chromium, incluindo mudança de sessão, sala pessoal, chat móvel, amigos, ações reais de entrada e perfil. `pessoal-web.cjs` voltou a passar em Chromium e WebKit após a consolidação de CSS e valida nome próprio distinto do remoto ao reabrir a vista. Os três guardas de propriedade/duplicação de classes passaram; o guarda da nota agora calcula o contraste WCAG dos tokens resolvidos, exigindo 4,5:1 sobre fundo, painel e seleção.

Limites: o foco acima é verificado no navegador de bancada; não reproduz teclado virtual real do iOS/Android nem janela nativa Windows. Os conteúdos internos de MODs externos e as permissões do sistema operacional dependem de suas próprias provas. Nenhum teste desta rodada capturou ou reproduziu som físico.

## Janelas baixas e cancelamento de recepção

`altura-600/` registra a verificação focada de ações em 320/390/768 × 600: conversar por link na entrada, conectar/código/amigos/perfil, mic/som/saída da chamada, compositor/saída pessoal e rodapés de preparar/pré-conferência. Cada ação é rolada até a vista e medida por bounding box e `elementFromPoint`; nenhuma correção escondeu overflow. A primeira rodada revelou os controles da chamada ausentes entre 701 e 900 px: a lateral recolhia até 900, mas seus controles alternativos só apareciam até 700. O limiar foi alinhado e apenas as três chamadas foram repetidas. Em 320/390 o seletor inicial dependia do rótulo dinâmico e foi trocado pelo vínculo estável da ação. `relatorio-combinado.json` junta as 15 cenas aprovadas da primeira rodada às três chamadas aprovadas da revisão. Os três inícios canônicos também foram recapturados e não têm o hint inferior duplicado.

`chamada-web-recepcao.cjs` passou 7 provas Node com WASM de produção e Ed25519 reais. As duas novas controlam a Promise de Registrar: desligar não reinstala uma recepção quando a resposta chega tarde; OFF → ON espera sua revogação e cria outro convite. A caixa cancelada não faz polling/renovação e a revogação antiga não encerra uma nova chamada. O encontro/reloginho dessas provas são controlados, sem mídia física.

## Limpeza de textos e perfil compacto

A rodada atual remove descrições repetidas e compacta o perfil/agenda. O mapa completo, incluindo itens mantidos, está em [mapa-microtextos](../0.17.4/mapa-microtextos.md). Testes atuais: 348 de interface e 11 guardas mobile aprovados.

As capturas estreitas da bancada `visual-0174` são ensaios responsivos da casca desktop, não a casca web mobile. A galeria as identifica assim; `mobile-limpo` reúne capturas da casca celular com dados demonstrativos. `perfil-compacto` contém as 16 provas de agenda vazia/preenchida, e `escala2` testa controles com texto ampliado.
