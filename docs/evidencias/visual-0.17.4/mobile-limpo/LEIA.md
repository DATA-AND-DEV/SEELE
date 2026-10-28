# Casca mobile limpa — 0.17.4

Quatro capturas em **390 × 844**, Chromium com toque e `data-casca="celular"`: [início](inicio.png), [perfil](perfil.png), [sessão](sessao.png) e [chamada](chamada.png). São as estruturas e os renderers da casca celular, e não o desktop estreitado.

O [roteiro](roteiro.cjs) reaproveita `navegacao-0-17.cjs`, com `casca: "celular"` e respostas de IPC/dados simuladas. Não há conexão com servidor SEELE ou pares, microfone, áudio ou captura física. O código, a disponibilidade de recepção e a conversa são fixtures; estas imagens não homologam transporte ou disponibilidade real de rede.

O [relatório](relatorio.json) confirma a casca efetiva e zero erros nas quatro cenas. A [verificação](verificacao.json) registra os controles executados: nenhuma telemetria/uptime/duração/badge no DOM mobile, nenhum rodapé fictício, nenhuma das dicas padrão retiradas, nenhum transbordamento horizontal e nomes acessíveis nos controles visíveis. As imagens também foram inspecionadas. O sinal individual e o estado da voz continuam presentes; as métricas de atraso/jitter/perda/uptime e seus espaços foram removidos.

Reproduzir na raiz do repositório, com uma instalação existente de Playwright:

```sh
PLAYWRIGHT=/caminho/node_modules/playwright node docs/evidencias/visual-0.17.4/mobile-limpo/roteiro.cjs
```

A auditoria usa `String.raw` para preservar a expressão de separação dos IDs em `aria-labelledby`. Uma primeira execução do auditor perdeu essa barra de escape e gerou falso positivo no campo de código do perfil; a fonte do produto permaneceu inalterada e a execução final passou.
