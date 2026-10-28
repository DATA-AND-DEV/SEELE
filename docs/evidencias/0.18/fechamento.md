# SEELE 0.18 — fechamento de validação local

Rodada de 28/10/2026. Versão de empacotamento: **0.18.0**. Nenhum commit, push, publicação, troca de download, assinatura ou alteração de DNS nesta rodada. Este relatório não substitui a conferência dos instaladores finais.

## Entregas e evidências

- **Links antigos:** novos convites rejeitam origens e pontos de encontro aposentados; registros locais migram offline sem perder identidade/pin/endereço. Revisão independente não encontrou defeito adicional. [Detalhes JavaScript](links-js.md) e [infraestrutura pública](dominios-infra.md).
- **Código e chamadas:** navegador/desktop usam a identidade persistente e a configuração canônica do protocolo. Prova Mac Tauri/WebView real, IPC real e navegador passou nos dois sentidos por código, chat, áudio sintético e vídeo H264 Mac→web; parar e recomeçar a transmissão descartou os quadros anteriores. Perfis e mídias eram descartáveis; não foi captura física nem teste 5G.
- **Windows:** compilação da cópia autorizada e 348 testes frontend passaram. A interface real WebView2 abriu sem recorte, carregou WASM e respondeu via IPC. O mesmo perfil descartável preservou código e chave em duas aberturas. C:\\SEELE, aplicativo instalado, perfil real e firewall não foram alterados. A prova final WebView2 ↔ Edge no mesmo Windows passou com IPC real: chamada por código e aceite nos dois sentidos, mensagens bidirecionais em cada chamada, áudio sintético enviado/decodificado nas duas pontas e captura encerrada ao sair. Sinalização local interceptada para encontro Rust e mídia RTC real, sem TURN nem acesso à produção. [Evidência Windows](windows.md).
- **Interface e sites:** auditorias de larguras, controles, menu, agenda, perfil, hospedagem e documentos estão na [integração](../0.17.4/integracao.md) e em [sites-final.md](sites-final.md). Não redesenha automaticamente conteúdo interno de MODs de terceiros.

## Testes executados

| Verificação | Resultado |
| --- | --- |
| Core com OpenH264 local exigido | 460 passaram; sem ignorar o codec |
| Protocolo | 441 passaram |
| Servidor | 673 passaram |
| Web/WASM após configuração canônica | 76 passaram |
| Frontend / mobile / fontes / tokens | 348 / 11 / 5 / 4 passaram |
| Contratos da ponte web | 16 passaram |
| Ponte JavaScript | 23 passaram |
| Clippy dos pacotes alterados, testes, avisos como erro | Passou |
| Empacotamento web | 6 testes passaram |
| Carregamento com CSP/headers e atualização de cache | Chromium e WebKit passaram; identidade preservada |
| Chamadas completas de serviço | Chromium e WebKit passaram: convite, aceite/recusa, texto, perda, cancelamento, revogação e nova entrada |
| Ensaio de 15 minutos, por motor | Chromium e WebKit passaram, 31 amostras cada, áudio sintético progredindo e chat bidirecional |

O ensaio longo precedeu a centralização final dos endereços; o transporte de mídia permaneceu igual. O fluxo completo e a prova nativa foram repetidos após a configuração final, com WASM de 2.215.066 bytes, SHA-256 `be69ec49e09af26a45fdaa480c70d2192ad91f058a11f9a5033cde350b02eefc`. Isso não equivale a dizer que o binário final foi submetido a todos os ensaios físicos.

A prova integrada de interface encontrou uma falha no primeiro ATENDER do WebKit. A investigação reproduziu clique em ENTRAR durante o registro inicial do código. O conserto aguarda o preparo e invalida continuações canceladas; regressão controlada passou nos dois motores e falhou com o conserto removido. Cancelar durante a preparação também passou sem reabrir o convite. A integrada completa final passou em Chromium/WebKit. Uma ocorrência anterior, distinta, de formulário inicialmente oculto não voltou a ocorrer em 10 páginas/contextos WebKit novos, sem repetir clique. Helper e sequência pointerdown/up/click estavam presentes; o redesenho observado ocorreu antes do gesto. A causa histórica não está comprovada e a ocorrência não foi atribuída à corrida corrigida.

## Xcode: resultado e limite

- Tela cheia nativa, rotação e saída: **passou** no iPhone simulado.
- Ciclo curto de microfone: **passou**, incluindo desligar, pagehide e encerrar/reabrir o Safari sem reativação automática.
- Estabilidade da captura de 30 s: **falhou**; o Safari encerrou a track perto de 22 s.
- Controle de 45 s sem SEELE: **também falhou**, evento `ended` do navegador, nenhuma chamada a `stop()` e nenhuma amostra de áudio. Essa evidência separa o problema de captura do simulador do motor do SEELE, mas não homologa o microfone físico.

O Xcode travou na finalização do resultado combinado de 45 s/3 s. O processo foi encerrado depois dos resultados de teste registrados; o bundle ficou incompleto. Logs e resumos foram preservados. Não declarar essa execução como suíte Xcode integralmente aprovada. [Investigação](microfone-safari-investigacao.md).

## Estado para publicação

A correção WebKit e a interoperabilidade solicitadas estão fechadas no código e nos testes automatizados. Os builds debug Mac/Windows e o pacote web foram regenerados com o ajuste final. **Não há ainda instaladores 0.18 assinados nem publicação.**

## Etapas de lançamento e limites

1. Gerar instaladores finais da mesma revisão 0.18.0, conferir payloads/versão e obter as assinaturas do atualizador no Terminal.
2. Na publicação coordenada, atualizar encontro/web/sites e conferir downloads; retirar os vínculos e registros antigos que ainda respondem. A auditoria não alterou produção.
3. Homologação física de áudio, seletor, Brave/5G e comportamento dos instaladores finais permanece distinta dos testes automatizados. O limite do simulador e a ocorrência isolada de formulário não reproduzida precisam constar do fechamento; não declarar que todos os ambientes reais foram aprovados.

## Limitações mantidas

Sem TURN/relay: a conexão direta pode falhar entre certas redes. Receber no navegador exige página aberta e executando. Apagar dados do site pode apagar código/agenda. Não há transferência de identidade da origem antiga. Queda da mídia requer nova chamada com aceite; recuperar recepção não reconecta automaticamente a mídia. Compartilhamento de tela depende da plataforma; celular web não transmite a própria tela e o caminho alternativo WebKit não envia som de sistema.

Mac debug normal recompilado depois do patch, sem scripts de prova incorporados. O pacote web foi regenerado; carregamento/CSP passaram nos dois motores e os 348 testes frontend + 16 de contratos web passaram após o ajuste. Ainda não são instaladores assinados. Linux não foi executado nesta rodada.

## Recursos

Foram reutilizados checkout e caches. Os bundles temporários grandes de ensaios Xcode já diagnosticados foram removidos após preservar logs/resumos; não foram removidos fontes, evidências anteriores nem releases. A galeria local permanece aberta. Veja `limpeza-temporarios.json`.
