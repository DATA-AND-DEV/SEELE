# Roteiro de execução para Claude

Entrada principal: [contrato de arquitetura](../contrato-mods-api-propria-para-claude.md). Contratos operacionais: [API, mensagens e fluxos](contratos-e-fluxos.md).

**Este roteiro orquestra uma implementação futura. Nenhuma etapa abaixo foi executada ou aprovada por ter sido escrita.** Claude deve adaptar a organização interna ao checkout encontrado, mantendo os comportamentos e as provas exigidas. Não precisa pedir confirmação para escolhas rotineiras já contidas neste escopo.

**Continuação após `1900b37`:** consultar a [diretriz após a sonda E1](diretriz-apos-sonda-e1.md) e o [registro de execução](registro-de-execucao.md). As tabelas iniciais abaixo não substituem esse registro. E2 pode começar com autoridade, geração e registro independentes do executor; seu aceite integrado depende de E1 resolvida. O próximo candidato de execução é QuickJS nativo, sujeito às medidas especificadas na diretriz.

## 1. Antes de editar

1. Ler `AGENTS.md`, quando existir no caminho, e o `CLAUDE.md` do repositório. Registrar HEAD, branch e alterações locais de cada repositório envolvido. Não descartar trabalho em andamento.
2. Ler o contrato principal, este roteiro e o anexo de contratos. Em seguida, conferir os símbolos da tabela abaixo no código atual. Se o checkout mudou, atualizar o mapa antes de implementar, sem tratar números de linha antigos como autoridade.
3. Conferir releases e versão instalada antes de diagnosticar comportamento de distribuição. O teste de memória realizado aqui usou **0.11.2/API 2**; o código que este roteiro analisa já contém Workers. Não confundir os dois.
4. Produzir a matriz funcional dos três MODs com base no código anterior e atual. As versões locais 2.0.0 foram reduzidas a apresentação de leitura pela migração; elas não são a referência completa de funcionalidade.

## 2. Mapa do código conferido em `23fc654`

| Local/símbolo existente | Trabalho necessário |
| --- | --- |
| `apps/seele-app/src/main.rs`: `Session.connection`, `Session::connection` | Acrescentar propriedade de sessão/geração e registro de instâncias; não resolver trabalhos antigos contra o slot global atual |
| Mesmo arquivo: `Bridge`, `EventListener::on_event` | Vincular eventos à conexão/geração que criou o listener. Hoje `Bridge` contém somente `AppHandle` e emite eventos globalmente |
| Mesmo arquivo: `connect`, entrada hospedada, `desmontar_o_cliente`, `desmontar_a_sessao`, `disconnect` | Capturar a geração desde a tentativa de conexão; revogar antes de destruir conexão e descartar antes de mostrar outra sessão |
| Mesmo arquivo: `codigo_do_mod` | Preservar validação do ID/hash e bytes. Entrega do código precisa pertencer a uma montagem revogável |
| Mesmo arquivo: `mod_request`, `snapshot`, `invoke_handler` | Fachada restrita de MODs, validação de autoridade, canal e limites; não entregar o conjunto geral de comandos à execução de terceiros |
| `apps/seele-app/build.rs`, `capabilities/janela.json`, `tauri.conf.json` | Provar os limites reais de IPC/origem/CSP; permissões dos plugins não bastam para os comandos próprios |
| `apps/seele-app/ui/base.js`: `PRELUDIO_DO_MOD`, `montarOMod`, `carregarMods` | SDK/instância com geração e carga cancelável. Validar cada conclusão assíncrona; presença de um ID no Map não identifica uma geração |
| Mesmo arquivo: `pedirAoServidor`, `pedidosDeMod`, `ouvirMod` | Correlação de pedidos com sessão/instância. Pedido numérico repetido em outra conexão não pode receber resposta antiga |
| Mesmo arquivo: `atenderOMod` | Checar a instância **antes** de `pedido`, `snapshot`, `regiao` ou `tema`; hoje a checagem de Worker atual está em `responder` |
| Mesmo arquivo: `montarODeclarado`, `desenharARegiaoDoMod` | Renderer incremental, propriedades validadas, eventos por IDs e limites no receptor; profundidade sem limite de largura não basta |
| Mesmo arquivo: `temaDosMods`, aplicação/escrita de tema, `limparARegiaoDoMod` | Camadas confinadas à sessão e composição determinística; remover a camada inteira sem sobrescrever preferências pessoais |
| Mesmo arquivo: `encerrarOAmbienteDosMods` | Delegar para um ciclo de vida único e idempotente, incluindo recursos do anfitrião |
| `apps/seele-app/ui/tela-sessao.js`: `ejetar`, `sairDoServidorParaAEntrada`, listener de `Ended` | Encerrar também na saída local. Atualmente `ejetar` espera `disconnect` antes de chamar a limpeza JS; revisar a ordem para revogação imediata, sem abrir janela para novos efeitos durante a espera |
| `apps/seele-app/ui/index.html`, `camada-mods.js` | Montagem do renderer e superfícies; gestão deve distinguir carregado, pronto, suspenso, recusado e encerrado |
| `crates/seele-ffi/src/lib.rs`: `Connection::mod_request`, `Connection::disconnect`, eventos `ModReply` | Manter ponte para o core; descarte e geração precisam sobreviver às filas e listeners. `disconnect()` já marca `running=false` e envia `Shutdown` |
| `crates/seele-core/src/client.rs`: `mod_request`, transferência de arquivos/`ModVolume` | Reaproveitar transporte, sem levar QUIC para a interface. Há envio de volume, mas o ramo receptor `StreamType::ModVolume` registra que ainda não há consumidor |
| `crates/seele-server/src/mods/volume.rs`, `despacho.rs`, `pedidos.rs` | Preservar autorização da transferência por pessoa/MOD e a separação controle/bytes. Completar o fluxo necessário à mídia em vez de presumir que a fachada já está pronta |
| `crates/seele-server/src/mods/mod.rs`, `persistence/mods.rs` | Preservar regras, dados e isolamento de servidor. Desconectar cliente não equivale a desligar MOD no servidor |
| `crates/seele-proto/src/mods.rs`, `api/`, `xtask/src/check_api.rs` | Versionar o contrato e conferir constantes. Neste checkout o comentário diz API 3, mas a constante vale 2; não publicar essa divergência |

**Armadilha de `Arc`:** capturar `Arc<Connection>` protege contra escolher a conexão errada, mas pode manter a antiga viva. Revogar os pedidos, chamar o encerramento explícito pelo dono e liberar as referências das tarefas; não depender apenas de tirar o `Arc` do slot de `Session`. A geração antiga também não pode emitir um `Ended` que derrube a nova.

## 3. Organização interna sugerida

Novos nomes abaixo são uma proposta de divisão, não arquivos encontrados no checkout:

- `apps/seele-app/src/mods_sessao.rs`: estados, identidade, autoridade, registro nativo e cancelamento.
- `apps/seele-app/ui/mods-runtime.js`: instâncias, mensagens, SDK, filas e lifecycle.
- `apps/seele-app/ui/mods-renderer.js`: superfícies, nós, lotes, eventos e liberação de recursos visuais.
- Um módulo de mídia confiável separado se o renderer crescer; código do MOD nunca entra nele.

Manter `src/mods.rs` concentrado na responsabilidade de pacotes/instalação que já tem. Evitar acrescentar todo o runtime aos arquivos `main.rs` e `base.js`, mas conservar adaptadores nos pontos existentes enquanto necessário. Não introduzir framework global ou reescrever a interface do produto para realizar esta entrega.

Se houver invariantes reutilizáveis no core, expô-las através da FFI. Não criar uma dependência direta da casca no protocolo. A exceção de dependência em `seele-server` existe para hospedar, não para contornar as fronteiras de cliente.

## 4. Etapas com dependências e condições de avanço

| Etapa | Depende de | Entrega concreta | Prova antes de avançar |
| --- | --- | --- | --- |
| E0 — inventário | Nada | Mapa atualizado, referência de versões, matriz funcional e registro de lacunas | Origem de cada afirmação identificada como código, medida ou proposta |
| E1 — executor e autoridade | E0 | Executor Worker com fronteira demonstrada, ou resultado documentado que exija QuickJS; protocolo de instância | Tentativas diretas de IPC/armazenamento/comunicação fora da API recusadas; loop não prende a saída; recursos descendentes sob controle |
| E2 — sessão e encerramento | E0 para infraestrutura independente; E1 resolvida para aceite integrado | Registro nativo/visual, geração, cancelamento e listeners vinculados | Saída local/remota, montagem atrasada e troca A→B; nenhum efeito antigo admitido; repetir com o executor escolhido |
| E3 — renderer e SDK mínimos | E2 | Superfície, layout, campo, botão, eventos, atualização incremental, tema | Formulário editável salva no servidor sem perder foco; camadas somem ao sair |
| E4 — desenho e mídia | E3 | Canvas/comandos, arraste, arquivos, áudio, imagens e recursos com dono | Tabuleiro simples, mídia em execução, carga/decodificação atrasada e saída; consumo medido |
| E5 — funções completas | E4 | Integração dos três MODs e uma experiência nova usando as primitivas | Matriz funcional concluída; cliente e servidor reais exercitados; sem substituir funções por avisos de indisponibilidade |
| E6 — publicação preparada | E5 | Contrato versionado, guia, exemplo, indexador, relatório e pacotes revisáveis | Mesma versão/capacidades nos produtores e consumidores; matriz nativa e regressão de desempenho documentadas |

Não passar E1 com base em um objeto JS congelado. Não passar E2 com base em `terminate()` sem olhar recursos do anfitrião. Não passar E5 com testes que só verificam que o cliente mostra texto.

Quando faltar uma máquina ou uma capacidade real, registrar a prova pendente e continuar o trabalho independente. Não marcar a matriz como verde, não trocar silenciosamente o objetivo por uma versão reduzida e não adotar um novo motor de navegador para esconder a limitação.

## 5. Recuperação dos três MODs

| Repositório | Fontes a editar | Provas funcionais necessárias |
| --- | --- | --- |
| `/Users/dev-alexandre/SEELE-MOD-MESA` | `ferramentas/interface.js`, `ferramentas/mesa.js`; gerar `cliente/main.js` por `build.mjs` | Campanha, edição de ficha, dados, cenas/mapas, peças/arraste, compêndio, iniciativa, imagens e ambientação; permissões de GM/jogador |
| `/Users/dev-alexandre/SEELE-MOD-PERFIS` | `ferramentas/interface.js`, `ferramentas/perfis.js`; gerar `cliente/main.js` | Edição, persistência, avatar/banner e animação, cartões integrados às pessoas; estado de voz e moderação preservados |
| `/Users/dev-alexandre/SEELE-MOD-ESTILO` | `ferramentas/interface.js`, `ferramentas/estilo.js`; gerar `cliente/main.js` | Editor/prévia, paletas, tipografia, densidade, bordas, fundos e efeitos; restauração ao sair |

`cliente/main.js` é gerado nos três repositórios; editar somente o bundle perde a mudança no próximo build. Os testes `cliente-api3.test.cjs` atuais cobrem a adaptação reduzida e precisam evoluir para os comportamentos recuperados. Ler as versões anteriores no Git/pacotes históricos para reconstruir a matriz, sem alterar os bytes dos pacotes já publicados.

As operações e o armazenamento do servidor são referência de continuidade. Se o uso do novo transporte exigir adaptação de mídia, preservar IDs, autorização e dados existentes, e escrever a migração necessária. Não introduzir reset do banco como requisito para testar.

## 6. Guia, catálogo e versão

O guia do catálogo fica em **SEELE-MODS-INDEXER**, não no repositório SEELE-SITE:

- Fonte: `/Users/dev-alexandre/SEELE-MODS-INDEXER/docs/guia-criacao-mods.md`.
- Gerador: `ferramentas/gerar_guia.py`; saída em `site/guia`, depois incorporada à distribuição do catálogo.
- Manifesto: `ferramentas/manifesto.py`; `VERSAO_DA_API` vale **3** no checkout consultado, divergindo da constante **2** do SEELE.
- Testes: `ferramentas/testes/test_manifesto.py`, `test_catalogo.py`, `test_guia.py`, `test_documentacao.py`; testes de interface em `site/testes`.
- Exemplo distribuído: `site/guia/exemplos/contador`; substituir/ampliar a prova de leitura com interação real e descarte, mantendo um pacote reproduzível.

Não editar uma fachada já publicada em `api/`. Conferir o que foi publicado antes de decidir se a API 3 ainda pode ser completada ou se precisa de nova versão. Registrar uma tabela com API de autoria, parser do cliente, parser do indexador, capacidades, versão do pacote e recusa esperada para cada combinação.

Preservar a verificação de hashes e o caráter histórico dos pacotes em `publicado/mods`. Preparar guia, exemplo, vetores e artefatos em conjunto. A execução deste roteiro não é autorização automática para publicar releases, trocar chaves ou substituir pacotes históricos.

## 7. Validação ligada às mudanças

Os comandos abaixo existem no projeto; executá-los nas etapas em que os componentes correspondentes forem alterados. Teste estático e mocks não substituem prova no app nativo.

```sh
cargo xtask check-deps
cargo xtask check-api
cargo test -p seele-app --test permissoes
cargo test -p seele-app --test frontend
cargo test -p seele-app --test encerramento
cargo test -p seele-proto mods
cargo test -p seele-server mods
```

Acrescentar testes comportamentais para o novo runtime, renderer e FFI. Os guardas de fonte existentes, especialmente em `tests/frontend.rs`, ajudam a detectar regressões conhecidas, mas procurar uma string no arquivo não comprova autoridade, isolamento, cancelamento ou latência.

Em cada repositório de MOD, `package.json` fornece `build`, `check`, `test` e `test:ui`. Atualizar as expectativas antes de usar essas suítes como evidência de funcionalidade completa. Rodar a comparação de memória com as mesmas funções e os mesmos recursos; o app com interface só de leitura não é a mesma carga.

No indexador, executar os testes Python das ferramentas e os testes Node da interface envolvidos, além de regenerar o guia e verificar que uma nova execução não altera os artefatos. Conferir os vetores cruzados entre indexador e SEELE quando formato ou versão mudar. Não gerar assinaturas de distribuição com uma chave de produção para fazer um teste local passar.

## 8. Registro que Claude deve manter

Atualizar esta tabela durante a implementação, com links para código e evidências. Os estados iniciais são deliberadamente pendentes.

| Etapa | Estado inicial | Evidência exigida |
| --- | --- | --- |
| E0 | Pendente de reconferência no checkout de implementação | HEADs, alterações existentes e matriz funcional |
| E1 | Pendente | Relatório de fronteira de autoridade e executor escolhido |
| E2 | Pendente | Testes de corrida, saída e contadores de recursos |
| E3 | Pendente | Exemplo editável e renderer incremental |
| E4 | Pendente | Desenho, mídia, cancelamento e medidas |
| E5 | Pendente | Matriz dos três MODs e experiência independente |
| E6 | Pendente | Versões consistentes, guia gerado e matriz nativa |

Cada atualização deve dizer: o que mudou, em quais arquivos, que comportamento foi provado, o que ainda falta e qual etapa pode começar. Quando uma hipótese do documento não se sustentar no código, registrar a evidência e a adaptação; preservar isolamento por servidor, limpeza obrigatória, liberdade funcional e baixo consumo.
