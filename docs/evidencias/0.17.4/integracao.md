# SEELE 0.18 — integração local

Este lote foi promovido de 0.17.4 para **0.18** por decisão do usuário. Os caminhos das evidências anteriores são preservados para rastreabilidade.

Lote em desenvolvimento, sem commit, push, implantação ou instaladores publicados nesta rodada. Os três agentes de desenvolvimento usaram Sol 6.1 medium nos checkouts existentes. A versão pública não foi alterada.

## Implementação

- Identidade pessoal persistente no navegador, código, agenda, amigos, bloqueios e fixação da chave completa. Identidades com o mesmo nome permanecem distintas. A identidade anterior por servidor é preservada.
- Chamadas por código nos dois sentidos entre navegador e desktop, além de navegador ↔ navegador. Descoberta e envelopes assinados; nenhuma chave privada é exportada ao frontend nativo. VPS apenas para encontro/sinalização, sem TURN nem relay de mídia/chat.
- Renovação da recepção, recuperação limitada e retorno de suspensão sem reabrir o microfone. Corridas de descoberta, desligamento/registro e nova tentativa corrigidas.
- Áudio recebido sem exigir captura local; falha de foco cancela a captura pendente. Encerrar ou sair invalida permissões tardias.
- Captura WebKit por vídeo/requestVideoFrameCallback quando TrackProcessor não existe, reutilizando encoder H.264, protocolo e limites. Nesse caminho o som do sistema é indisponível; voz continua separada.
- Nova composição para início, perfil, servidores, hospedagem/configurações, chamadas, conversa pessoal, diálogos e navegação dos MODs. Produto, documentação e catálogo seguem o kit visual fornecido.
- WASM nativo gerado pelo build e conferido por hash das fontes/artefato; configuração CSP limitada às capacidades necessárias. CI recebe explicitamente o WASM de produção, sem depender de um arquivo local ignorado pelo Git.

## Evidência e limites

| Prova | Resultado |
| --- | --- |
| Registro/cancelamento | 7 provas determinísticas aprovadas, incluindo OFF e OFF→ON com registro pendente |
| Frontend Rust/JS | 348 aprovados na limpeza atual (350 antes da retirada de dois guardas de texto obsoletos); fontes 5 e tokens/contraste 4 na integração anterior |
| Clippy app/xtask e dependências | Sem advertências com `-D warnings` |
| Geração de tags do servidor | 3 testes aprovados |
| xtask | 107 unitários, 4 ADR, 5 avisos e 142 empacotamento aprovados; o único teste bloqueado pelo sandbox passou isolado fora dele (consulta sysctl) |
| Chamada completa Chromium/WebKit | Convite/aceite, chat e áudio sintético bidirecionais, permissão tardia, encerramento e reentrada aprovados |
| Captura Chromium/WebKit | H.264 codificado/decodificado, parar/reiniciar e geração antiga aprovados, tanto conversa pessoal quanto fachada de servidor |
| Tauri/WKWebView Mac real ↔ Chromium | Código nos dois sentidos, chat, áudio sintético, tela vermelha/azul, parar/reiniciar e limpeza aprovados |
| Interface principal | 30 cenas; 24 cenas adicionais; navegação MODs em 1280/390/320 |
| Janelas baixas | 18 cenas em 320/390/768 × 600; controles da chamada corrigidos entre 701 e 900 px |
| Hospedagem/configurações | 28 cenas aprovadas em 1280/390; guardas de cópia/migração preservados |
| Produto/docs/MODs | Produto 390/768/1440; docs/MODs 35 cenas em 320/390/720/768/1440, menus e controles verificados |

A prova nativa usou perfil descartável, endpoint local e instrumentação de captura sintética: canvas e osciladores silenciosos. O transporte, IPC de identidade, codecs e leitura de vídeo são os do produto. Não comprova seletor de tela físico, autorização do macOS, dispositivos reais de áudio ou qualidade pela internet. Os processos, perfil e executável fixado dessa prova foram removidos; as evidências pequenas foram preservadas.

A capacidade da conversa pessoal nova depende também das APIs de mídia do navegador/WebView instalado. Quando faltam, a interface informa a indisponibilidade; esta prova não homologa versões antigas do macOS/WebView2.

Faltam homologação deste lote no Windows instalado, Brave/iPhone físico por 5G, teclado virtual e permissões/captura físicas, e ensaio prolongado da nova chamada. Sem TURN, redes que impedem conexão direta ainda podem falhar. A mídia não reconecta automaticamente após queda; uma nova chamada requer novo aceite. Receber pelo navegador exige página aberta/ativa o suficiente para executar; apagar os dados do site pode apagar identidade e agenda.

O catálogo assinado, revogações e pacotes de MODs permaneceram intactos. Novas telas de MODs são da casca e do site; a aparência interna de MODs externos depende do próprio MOD. Capturas com pessoas de demonstração não são medições de uma sessão real.

Veja também [provas visuais](../visual-0.17.4/LEIA.md), [guia em desenvolvimento](../../usar-a-0.18.md) e os JSON/logs nesta pasta.

## Revisão de textos e espaço — estado local atual

Removidas microdescrições redundantes, avisos educativos permanentes e métricas da casca celular. Perfil e agenda compactados, com controles testados em altura baixa e texto ampliado. A rodada atual passou 348 testes de interface e 11 guardas mobile; dois testes antigos que exigiam textos removidos foram aposentados por mudança explícita do requisito visual. Erros reais, permissões, consentimento, backup e migração continuam visíveis quando aplicáveis. Consulte [o mapa](mapa-microtextos.md). Sem commit ou publicação nesta rodada.
