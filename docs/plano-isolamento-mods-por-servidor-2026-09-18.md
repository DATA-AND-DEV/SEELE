# Remapeamento e plano de isolamento dos MODs por servidor

Data: 18/09/2026. Escopo: análise e planejamento; nenhuma correção de produto aplicada.

## 1. Conclusão

O contrato desejado é: **o servidor instala, escolhe versões e guarda o estado dos MODs; o cliente baixa os componentes necessários e os executa somente durante a sessão daquele servidor. Ao sair, o cliente recupera sua aparência e comportamento próprios.**

O código já implementa parte desse contrato: conjunto habilitado e dados chave/valor são guardados no banco do servidor. Entretanto, execução na janela, arquivos dos MODs e seleção dos pacotes ainda atravessam fronteiras que deveriam ser por instância ou por sessão.

O problema da cor verde tem uma explicação concreta no código atual: o ESTILO modifica os tokens da raiz global da página, mas o caminho de saída voluntária não garante o evento usado para restaurá-los. **Não encontrei no ESTILO examinado persistência do tema em `localStorage`: ele já salva `dados.theme` no servidor.** Portanto, mover a configuração para o servidor, sozinho, não corrige esse defeito.

Há também problemas independentes: arquivos mutáveis compartilhados entre servidores, substituição global de versões, falta de identidade lógica de servidor no contexto dos MODs e ausência de isolamento real do JavaScript em relação ao aplicativo.

### Base e limites da análise

- SEELE: checkout com HEAD `e2cbd67`, incluindo o estado de trabalho existente durante a leitura.
- Fontes locais de SEELE-MOD-ESTILO (manifesto 1.0.1), SEELE-MOD-PERFIS, SEELE-MOD-MESA e pontos de publicação/validação do SEELE-MODS-INDEXER.
- Foram observadas alterações preexistentes em `crates/seele-server/src/mods/mod.rs`, `pedidos.rs` e o arquivo novo `volume.rs`. Não foram modificadas. A implementação de volume está em andamento e deve ser integrada ao novo escopo de armazenamento.
- Documentos anteriores foram usados como contexto, mas os achados abaixo se apoiam no código atual. Há correções recentes já presentes: download após aceite, indicação de MOD obrigatório, rascunho e aplicação do conjunto em transação, reconexão preservando hospedagem.
- Foi executado um experimento em Node com o trecho real de `carregarMods`, substituindo DOM e ponte por dublês: após carregar um MOD, fazer `snapshot` retornar `NotConnected` deixou **1 script ativo antes e 1 depois**. Isso confirma a falta de limpeza nesse caminho; não constitui reprodução do aplicativo nativo nem prova da versão usada no relato.
- Não foram executadas sessões nativas host/convidado, builds Windows/macOS, alterações em bancos reais, publicação ou migração de dados.

## 2. Arquitetura atual, de ponta a ponta

| Camada | Responsabilidade atual | Escopo efetivo e problema |
| --- | --- | --- |
| Launcher | `apps/seele-app/src/versoes.rs`, `crates/seele-lancador/src/dados.rs` e resolução de executáveis iniciam a versão escolhida com seu `SEELE_HOME` | Isolamento por versão do SEELE; não substitui isolamento entre servidores dentro daquela raiz. A semeadura pode copiar a estrutura antiga junto. |
| Registro de servidores | `apps/seele-app/src/servidores.rs` escolhe `<config>/servidores/<id>/seele.db`; legado usa `<config>/seele.db` | Banco separado por servidor, mas todos recebem `<config>/mods` ao hospedar. |
| Indexador/catálogo | SEELE-MODS-INDEXER valida manifesto, calcula hash e publica catálogo; `apps/seele-app/src/catalogo.rs` confere assinatura, revogações e conteúdo | Boa base de procedência; a instalação final ainda substitui um único diretório por ID. |
| Instalação local | `apps/seele-app/src/mods.rs::instalar_de` instala em `<config>/mods/<autor>/<nome>` | Uma versão disponível por ID na raiz de configuração. `dados/` mora junto do pacote e atravessa atualizações. |
| Descoberta | `crates/seele-core/src/mods.rs`, `crates/seele-ffi/src/mods.rs` listam diretórios e calculam hash | Lista global de pacotes mistura-se, na UI, com gestão do servidor. O hash exclui `dados/`, corretamente distinguindo bytes e estado, mas o diretório físico ainda os reúne. |
| Ativação | `main.rs::aplicar_conjunto_de_mods` → `persistence/mods.rs::definir_conjunto` | Tabela `mods` do banco selecionado; já há transação e comparação de conjunto-base, porém base vazia dispensa comparação. Não há ainda commit conjunto de banco e runtime. |
| Anúncio e aceite | `mods/anuncio.rs`, `seele-proto/src/mods.rs`, `seele-core/src/client.rs`, `seele-core/src/aceites.rs`, `camada-mods.js` | Hash do conjunto é anunciado; aceite é persistido por endereço canônico. Aceitar/download não prova inicialização do runtime. |
| Execução no servidor | `mods/mod.rs`, `despacho.rs`, `pedidos.rs`, `seele-server/src/lib.rs` | QuickJS para eventos carregado no boot; runtime novo por pedido API 2. As duas vias têm ciclos de vida diferentes. |
| Estado chave/valor | `persistence/mods.rs::ler_quintal/gravar_quintal` | `mod_data(mod_id,key,value)` já pertence ao banco do servidor. Não é preciso mover esse estado para fora do servidor. |
| Arquivos/volumes | `mods/arquivos.rs`, `mods/mod.rs::carregar_do_disco`, `mods/pedidos.rs` e evolução `volume.rs` | Recebem `<config>/mods/<id>/dados`; servidores da mesma instalação compartilham o diretório mutável. |
| Ponte cliente/servidor | `base.js::SeeleMods`, `main.rs::mod_request`, FFI e protocolo | Pedidos identificados por contador, ID do MOD e canal. O objeto global da API não fica preso a uma sessão; callbacks antigos podem usar a conexão nova. |
| Recursos do cliente | `main.rs` registra `mod://`; `apps/seele-app/src/mods.rs::hash_confere/serve` | Confere hash informado e arquivo declarado, mas essas funções não recebem autorização vinculada à sessão/servidor. |
| Execução na janela | `base.js::carregarMods` insere `<script type=module>` no documento principal a cada conjunto encontrado | Polling de 4 segundos, mapa por ID/hash, mesmo DOM e contexto JavaScript do aplicativo. |
| Descarregamento | `base.js` despacha `seele-mod-unload` ao receber `Ended` ou mudar o catálogo | Depende da cooperação do MOD; remover o elemento script não desfaz o código executado. Saída local e operações assíncronas não estão centralizadas. |
| MODs oficiais | ESTILO altera tokens globais; PERFIS modifica apresentação de pessoas; MESA cria UI, timers e áudio | Todos precisam obedecer ao mesmo ciclo de vida, não apenas ESTILO. PERFIS/MESA também usam arquivos no servidor. |

### Fluxo atual resumido

```text
Catálogo assinado → pacote global por ID em <config>/mods
                                     │
Servidor selecionado → banco próprio → mods habilitados + mod_data
                                     │
Anúncio/aceite → download → conexão → polling → script no DOM principal
                                     │
                                  ModRequest
                                     │
                    QuickJS → banco próprio + dados/ compartilhado
```

## 3. Achados e prioridades

### P0 — saída voluntária não encerra o ambiente dos MODs

Evidência: `ui/tela-sessao.js::ejetar` chama `disconnect` e troca as telas. `main.rs::disconnect/desmontar_o_cliente` remove a conexão; `seele-ffi/src/lib.rs::Connection::disconnect` envia `Shutdown`. O driver pode sair pelo encerramento local sem emitir `Event::Ended`. O descarregamento em `base.js` está condicionado a esse evento. O `catch` de `carregarMods` limpa exigências, mas não descarrega scripts.

No ESTILO, `ferramentas/estilo.js::setToken/apply` altera `document.documentElement`; `restore` só é executado pelo descarte cooperativo. Isso explica por que esconder a tela da sessão não remove o verde da entrada/launcher.

**Ação:** um único encerramento idempotente da sessão deve invalidar e desmontar MODs antes de mostrar qualquer tela fora do servidor. Invocá-lo em saída local, troca, término remoto, expulsão, revogação de aceite e falha definitiva. `NotConnected` deve ser uma defesa adicional, não o mecanismo principal.

### P0 — execução na janela inteira impede uma garantia forte de isolamento

O MOD compartilha `document`, objetos globais e o Tauri global (`withGlobalTauri: true`) com o produto. O wrapper `Object.freeze(SeeleMods)` não impede acesso por outros caminhos. A configuração de capacidades da janela não transforma comandos próprios em uma API isolada por MOD.

O evento de descarte só funciona quando o MOD o implementa corretamente. Timers, listeners, CSS, mutações em objetos globais, áudio e operações atrasadas podem sobreviver. Fazer cópia dos estilos e restaurá-los não resolve alterações arbitrárias em JavaScript.

**Ação:** adotar contexto de execução descartável e ponte limitada para a interface do produto. A correção imediata cooperativa deve ser explicitamente tratada como contenção, não como conclusão da arquitetura.

### P0 — arquivos mutáveis de servidores diferentes compartilham caminho

`main.rs::hospedar` passa a mesma raiz `config/mods` para bancos distintos. Tanto `carregar_do_disco` quanto `pedidos.rs` derivam `dir.join("dados")`. Assim, `arquivos.ler/escrever` de um mesmo MOD em A e B pode acessar os mesmos arquivos, ainda que `dados` chave/valor esteja corretamente separado em SQLite.

**Ação:** separar fisicamente pacote imutável de diretório mutável, este pertencente à instância. Aplicar a mesma raiz a pedidos, eventos, uploads, downloads, limpeza e backup.

### P1 — uma atualização substitui o pacote de outros servidores

`catalogo.rs::instalar_do_catalogo` chama `instalar_de(..., true)`, que publica no mesmo diretório por ID. Dois bancos podem exigir hashes diferentes, mas só um pacote fica disponível ali. A atualização pode deixar outros servidores exigindo conteúdo que saiu do disco. O comando `instalar_mod_do_catalogo` ainda reaplica automaticamente o MOD quando ele já é exigido pelo servidor hospedado.

**Ação:** armazenamento por conteúdo e referências explícitas por servidor. Download deve apenas disponibilizar bytes; selecionar/aplicar versão deve ser uma operação separada do servidor escolhido.

### P1 — identidade do servidor e identidade da sessão estão incompletas

Aceites são indexados por endereço. `servidores.rs::uma_chave_por_maquina` pode fazer bancos diferentes herdarem a mesma chave TLS do legado. Portanto, **endereço e fingerprint, sozinhos, não distinguem necessariamente servidores lógicos**. O slug local também não é uma identidade remota autenticada.

`modsCarregados` compara ID/hash; dois servidores usando os mesmos bytes não necessariamente geram um novo runtime se a limpeza falhar. A revisão do tema também é local ao servidor: uma revisão alta de A pode fazer um runtime sobrevivente ignorar B.

**Ação:** UUID persistente de instância no banco e anunciado pela conexão autenticada, associado à identidade TLS. Além disso, uma geração local nova para cada sessão impede reutilização de callbacks e recursos anteriores.

### P1 — corridas entre saída, carregamento e resposta

`carregarMods` tem vários `await` e não confere geração antes de aplicar o resultado. Um catálogo ou script iniciado em A pode concluir depois da saída. `SeeleMods.request` espera o listener e depois usa a conexão corrente; não carrega um contexto imutável de A.

**Ação:** toda tarefa captura o contexto que a criou; cada entrega valida se ele continua ativo. Cancelar pedidos, timers, listeners e downloads pertencentes à sessão; impedir que resposta antiga reinstale UI. Eventos e respostas da FFI também precisam de identificação da sessão de origem.

### P1 — runtime de eventos e runtime de pedidos podem divergir do conjunto

`seele-server/src/lib.rs` cria o despachante no boot a partir dos IDs então habilitados. `despacho.rs` consulta o conjunto a cada evento, mas isso não carrega automaticamente fontes novas. `carregar_do_disco` recebe IDs, sem exigir o hash persistido; já `pedidos.rs` verifica o hash a cada pedido. Ativar/atualizar durante hospedagem não tem uma única transição de runtime comprovada.

O caminho de eventos lê o quintal, solta o lock, executa e depois grava o mapa; o caminho de pedidos executa sob o lock. Há uma janela para sobrescrever alterações concorrentes de pedidos com o snapshot antigo de um evento.

**Ação:** supervisor por servidor e revisão de conjunto, resolução uniforme por hash e serialização/revisão das mutações por MOD. Não manter código antigo respondendo sob anúncio novo.

### P1 — entrada pronta não equivale a pacote aceito

O download após aceite já existe, e `load` do script é registrado, mas o próprio código reconhece que isso não comprova inicialização do MOD. O polling pode descobrir ausência/falha depois de a sessão já estar visível. Aceite previamente salvo também precisa passar pela preparação se o cache foi apagado ou está incompatível.

**Ação:** pipeline único para entradas novas, aceites anteriores, reconexões e host: consentir → resolver bytes exatos → verificar → preparar → obter estado inicial → montar → pronto. Falha de MOD obrigatório impede marcar a sessão como pronta.

### P2 — migração e instalação precisam proteger dados separadamente

O instalador atual move `dados/` do destino para a estufa antes de concluir a substituição. Existem falhas posteriores que removem a estufa; a intenção documentada de preservar dados exige rever todos os ramos de rollback. Não executar migração de dados pelo atual caminho genérico de substituir pacote.

**Ação:** migração própria, com backup consistente, diário de progresso, verificação e publicação final. Pacotes futuros não devem conter dados de runtime a transportar durante update.

## 4. Modelo de destino

### Quatro escopos separados

| Escopo | O que pertence a ele | Persistência |
| --- | --- | --- |
| Cliente | Identidade pessoal e preferências próprias do produto | Perfil do cliente; MOD não grava nele |
| Pacote | Código verificado, manifesto, versão e hash | Cache imutável reutilizável; não concede ativação |
| Servidor | Conjunto instalado/ativo, versões fixadas, configuração, estado, arquivos e permissões | Banco e diretório da instância hospedada |
| Sessão | Runtime do componente cliente, UI, tema aplicado, listeners, timers, áudio, pedidos e rascunhos | Efêmero; revogado ao terminar a sessão |

Uma tela de configuração pode continuar no cliente: ela é uma interface administrativa que pede ao servidor para salvar. O problema é a autoridade e a persistência, não a localização visual do formulário. O servidor verifica permissão em cada operação. No ESTILO isso já existe com `ctx.admin`; deve ser preservado.

Preferências pessoais oferecidas por um MOD, quando necessárias, pertencem ao servidor e à pessoa autenticada (`server_instance_id + mod_id + person_id`), não ao perfil global do aplicativo. Rascunhos não salvos morrem com a sessão, salvo contrato explícito diferente.

### Layout proposto

```text
<raiz de dados compatível com a versão do SEELE>/
  client/                                  # estado próprio do produto
  mod-packages/<content-hash>/              # somente código/manifesto verificados
  servers/<instance-id>/
    seele.db                               # mods, mod_data, revisões, permissões
    mod-data/<autor>/<nome>/                # arquivos mutáveis deste servidor
    backups/
  mod-consents/                            # autoridade + instance-id + conjunto
```

Os nomes são proposta; o banco legado pode continuar no lugar atual com um mapeamento explícito de raízes. Não é necessário mover todas as bases para adotar o modelo. O banco é a fonte única do conjunto fixado; se existir arquivo de lock para exportação, ele é derivado, não uma segunda autoridade.

O cache pode ser deduplicado entre versões do SEELE posteriormente. A primeira entrega deve preservar a política atual do launcher de dados por versão, sem compartilhar silenciosamente bancos ou arquivos mutáveis entre executáveis incompatíveis.

### Contexto obrigatório

```text
SessionModContext = {
  authenticated_server_identity,
  server_instance_id,
  session_generation,
  mod_set_revision / mod_set_hash,
  mod_id,
  package_hash
}
```

O backend cria esse contexto e associa permissões à conexão. O MOD não pode escolher livremente outro `mod_id`, servidor ou geração. Ao sair, a geração é invalidada primeiro; qualquer comando atrasado é recusado antes de tocar a conexão ou banco seguinte. O servidor valida a pessoa e as permissões reais, nunca um `admin` informado pelo frontend.

### Ambiente de execução e aparência

Recomendação: lógica de MOD em contexto separado e terminável, com UI própria isolada e contribuições à interface principal feitas por API do produto. A escolha entre worker/runtime e webview/frame deve ser fechada por um protótipo de Tauri em macOS e Windows. O resultado exigido é ausência de acesso ao DOM global, IPC geral e armazenamento global; um iframe de mesma origem ou Shadow DOM, isoladamente, não oferece essa garantia.

Para personalização ampla, fornecer APIs de tema e de apresentação de canais/pessoas e regiões de UI. O servidor continua podendo personalizar a experiência, mas o produto é dono dos recursos e de sua desmontagem. Alterações de base exigem extensão explícita da API.

O tema deve ser uma camada declarativa sobre o contêiner da sessão. A entrada, launcher e preferências locais permanecem fora dela. Diálogos renderizados fora desse contêiner precisam de raiz temática explicitamente associada à mesma sessão. Ao desmontar, remove-se a camada inteira e reaparece a preferência atual do usuário, sem fixar uma cor padrão que sobrescreva personalização legítima.

Isso requer revisar o ADR 0045, que hoje permite acesso irrestrito à janela. Manter JS arbitrário no contexto principal e prometer ausência de efeitos fora do servidor são objetivos incompatíveis. A política nova deve ser documentada como mudança de contrato e acompanhada de migração dos MODs.

## 5. Ciclo de vida proposto

```text
Fora do servidor
  → autenticar e identificar instância
  → conferir anúncio e compatibilidade
  → obter consentimento para o conjunto, quando necessário
  → obter/verificar pacotes exatos
  → preparar runtimes e estado inicial
  → montar recursos da sessão
  → sessão pronta
  → invalidar geração
  → cancelar pedidos e desmontar recursos / terminar runtimes
  → fora do servidor
```

- Nenhuma execução de terceiros antes do aceite; cache existente não significa consentimento.
- Encerramento é idempotente e cobre saída local, erro, expulsão, troca de servidor, atualização do conjunto e fechamento/recriação da janela.
- Uma reconexão de transporte não deve permitir operações de MOD enquanto sua autoridade não foi revalidada. A implementação inicial pode recriar o runtime; só preservar estado efêmero se a continuidade de instância/conjunto estiver comprovada.
- Limpeza visual imediata não espera timeout de rede, conclusão de upload ou cooperação do MOD. Recurso que não desmonta é destruído com seu contexto.
- Revogar consentimento durante a sessão retira a autorização em vigor e conduz à saída; não deixa o MOD continuar executando até a próxima entrada.
- Um sinal de prontidão do cliente coordena UX; não é prova de que um cliente remoto não modificado ou malicioso realmente executou o código. Regras do servidor continuam sendo impostas no servidor.

## 6. Plano de implementação, em ordem

| Etapa | Mudanças e arquivos principais | Critério para concluir |
| --- | --- | --- |
| 1 — Conter o vazamento visual | `ui/base.js`, `tela-sessao.js`, `tela-fim.js`, `main.rs`, FFI; função central de término, geração, descarte de pendências e limpeza em `NotConnected`; ESTILO usa raiz da sessão | Sair, trocar e cair restauram a aparência; tarefa atrasada não remonta MOD. Registrar que legado ainda é cooperativo. |
| 2 — Identificar instância e separar raízes | `servidores.rs`, `hospedagem.rs`, `lib.rs`, `server.rs`, `persistence/schema.rs`, protocolo/FFI; UUID de instância e resolução de `package_root`/`data_root` | Dois bancos com mesmo MOD e mesma chave TLS mantêm estado/arquivos separados; UUID atravessa reinício. |
| 3 — Cache imutável e instalação por servidor | `core/mods.rs`, `ffi/mods.rs`, `apps/.../mods.rs`, `catalogo.rs`, comandos de instalação/gestão | A usa hash X, B usa Y; baixar/atualizar B não altera A. Download do convidado não instala nem ativa MOD em servidor que ele hospede. |
| 4 — Supervisor e aplicação consistente | `mods/mod.rs`, `pedidos.rs`, `despacho.rs`, `anuncio.rs`, `persistence/mods.rs` | Eventos e pedidos usam o mesmo conjunto/hash; atualização em execução funciona; falha mantém conjunto anterior íntegro. |
| 5 — Runtime cliente isolado e API nova | Extrair loader de `base.js`, criar gestor de sessão, ponte com contexto, APIs de tema/regiões/recursos; ajustar protocolo de recursos, CSP e capacidades | Destruir contexto elimina scripts e recursos sem depender de `unload`; MOD não alcança estado global do produto. |
| 6 — Gestão e preparação de entrada | `camada-mods.js`, telas de conexão/fim/servidores e comandos Tauri | Gestão indica servidor-alvo; participante vê exigidos; todo caminho de entrada prepara o conjunto antes de ficar pronto. |
| 7 — Launcher, migração e publicação | `versoes.rs`, `seele-lancador/src/dados.rs`, MODs oficiais, indexador, guias, contratos de API e release | Migração recuperável, matriz de versões explícita e pacotes novos publicados sem substituir versões existentes. |

As etapas 2–4 formam a fundação de armazenamento/runtime. A etapa 5 pode ser prototipada após definir o contexto da etapa 2, mas a implantação completa depende dos contratos estabilizados. A correção visual não deve esperar toda a reconstrução.

### Aplicação de conjunto no servidor

Preservar rascunho e SALVAR já existentes, mas receber seleção de `{id, package_hash}` com revisão-base obrigatória. Validar todos os pacotes e compatibilidade antes de publicar. Preparar novos runtimes sem permitir efeitos externos de inicialização; só habilitar capacidades na ativação. Publicar uma revisão e uma notificação, descartando recursos antigos.

A transação SQLite sozinha não torna atômicos memória, filesystem e efeitos externos de MODs. Usar supervisor com estado de preparação e registro de recuperação; depois de crash, reconstruir exclusivamente a revisão comprometida. Migrações de estado precisam de contrato próprio, cópia recuperável e validação antes da ativação. Não prometer rollback de chamadas externas já realizadas.

Para eliminar a corrida entre eventos e pedidos, escolher fila serial por MOD/servidor ou revisão otimista com rejeição de commit obsoleto. Preferir execução ordenada pelo supervisor sem manter lock global do banco durante operações lentas. Arquivos também exigem escrita temporária/publicação e tratamento explícito de falhas; o rollback do mapa `dados` não desfaz automaticamente arquivos.

### Experiência de gestão

- Launcher/servidores: “MODs deste servidor”, sempre identificando a instância selecionada, inclusive quando desligada. Nunca inferir o destino apenas de “estou hospedando”.
- Servidor selecionado: instalar, escolher versão, configurar, ativar/desativar e salvar alterações; autoridade verificada no backend. Administração remota só através de operação autenticada do servidor, não de comandos locais de hospedagem.
- Participante: lista exigida, versões, andamento e erros, opção de recusar/sair. Cache local não oferece switch que altera o conjunto do servidor.
- Configuração do MOD: mostrar apenas ações permitidas; leitura do tema não depende de poder editá-lo. Para MOD de escopo servidor, leitura não deveria exigir que exista um canal de texto selecionado; hoje a interface comum e a ponte associam pedidos a canal.
- Manutenção local: gerenciar espaço do cache sem chamar isso de configuração de MOD. Remoção de cache referenciado por servidor deve ser impedida ou exigir desinstalação explícita naquele servidor; apagar cache de convidado não apaga dados remotos.

## 7. Migração dos dados existentes

1. Inventariar todos os bancos registrados, inclusive legado, versões do launcher, linhas `mods`, `mod_data`, pacotes disponíveis e diretórios `dados/`. Registrar origem, hash e tamanho sem modificar os originais.
2. Gerar/persistir identidade lógica por banco. Backup restaurado como o mesmo servidor preserva UUID; clonagem para criar outro servidor gera UUID novo. Definir esse comportamento na importação.
3. Importar pacotes para cache imutável somente após verificar hash. Vincular cada banco ao hash que já exige. Se o pacote foi sobrescrito, tentar recuperar a versão exata do catálogo; se indisponível, bloquear aquele MOD e explicar, sem escolher a mais recente.
4. Preservar `mod_data` no banco ao qual já pertence. Não migrar tema do ESTILO a partir de CSS aplicado no cliente: esse CSS é uma projeção e pode estar obsoleto.
5. Tratar `mods/<id>/dados` como **origem compartilhada potencialmente ambígua**. Quando os estados dos MODs permitirem atribuição segura dos arquivos, migrar por referência. Se vários servidores puderem ser donos, manter cópia de preservação e pedir atribuição explícita no fluxo de migração. Não copiar todos os arquivos para todos os servidores: isso propagaria dados privados entre instâncias.
6. Construir destinos temporários, verificar conteúdos/referências, registrar conclusão e só então passar a usar as novas raízes. Falha/interrupção pode retomar sem apagar a origem. Backups SQLite devem ser consistentes com WAL; não copiar apenas `seele.db` enquanto há escrita.
7. Migrar aceites somente quando for possível associar inequivocamente endereço, autoridade e instância. Caso contrário, novo aceite na entrada; não ampliar um consentimento antigo para outro servidor.
8. Versionar layout e esquema de estado. O `state` do manifesto precisa passar de campo informativo a contrato de compatibilidade/migração quando usado. Downgrade não abre estado já migrado sem suporte declarado; usar a política de diretório/cópia anterior do launcher.
9. Limpar origens e cache não referenciado apenas depois de validação e política de retenção. Desinstalar MOD de um servidor preserva dados por padrão; apagar dados é ação separada e identificada.

## 8. MODs oficiais, API, indexador e releases

| Componente | Ajuste necessário |
| --- | --- |
| ESTILO | Manter tema e revisão no servidor; substituir acesso a `documentElement` por API de tema da sessão; edição administrativa e preview local limitado ao formulário; remover dependência de restauração de tokens globais. Alterar fontes em `ferramentas/` e regenerar pacote. |
| PERFIS | Manter perfis por pessoa no banco do servidor; arquivos e uploads na raiz da instância; integração de apresentação pela API; nenhum DOM/observer da sessão anterior. |
| MESA | Campanhas, imagens e retratos na instância; timers/áudio em recursos da sessão; fechar contextos de áudio e transferências ao sair; não transportar seleções efêmeras entre servidores. |
| Contratos | Versionar nova API/esquema conforme negociação, atualizar `api/v*.json`, `seele-proto/src/mods.rs`, conformance, documentação de criação e ponte. `reach` descritivo não deve ser tratado como mecanismo de autorização. |
| Indexador | Atualizar espelho de validação em `ferramentas/manifesto.py`, avaliações, compatibilidade, guia-fonte e exemplos; publicar novas versões/hashes, preservando acesso às versões antigas permitidas. |
| Launcher/releases | Informar compatibilidade antes de iniciar versão; testar semeadura e rollback com todos os servidores; divulgar mudança de API e migração. SEELE-SITE e SEELE-RELEASES são pontos de distribuição/comunicação, não devem virar fonte de estado de MOD. |

Não desativar silenciosamente a validação de assinatura/hash/revogação para facilitar a transição. MOD legado com acesso livre à janela não pode ser marcado como “isolado” apenas por declarar uma flag nova. Após a migração, só executar no modo novo pacotes compatíveis; manter versão antiga do produto como opção explícita de compatibilidade, sem afirmar que possui a garantia nova.

## 9. Matriz de validação e aceite

| Cenário | Resultado obrigatório |
| --- | --- |
| A aplica verde → sair pelo botão | Entrada/launcher recuperam aparência própria imediatamente; nenhum MOD de A em execução. |
| A aplica verde → encerrar host, expulsar, perder conexão definitivamente | Mesmo resultado de limpeza, com motivo correto. |
| A verde → B sem MOD | B e UI fora de sessão não herdam cor, botões, CSS, áudio, listeners ou estado de A. |
| A e B com mesmo ID/hash e temas distintos | Cada entrada cria contexto novo e obtém estado daquele servidor, mesmo com revisões iguais ou menores. |
| Catálogo, script, resposta ou upload de A conclui depois da saída | Resultado descartado; não altera UI ou dados de B. |
| MOD não implementa descarte, lança exceção ou deixa timers | Ambiente termina por ação do produto; app continua limpo. |
| Mesmo MOD com arquivo de mesmo nome em A e B | Conteúdos permanecem diferentes em leitura, escrita, update, limpeza e reinício. |
| A fixa versão X; B instala/atualiza Y | X permanece disponível e em uso por A; nenhuma reaplicação global. |
| Participante tenta salvar configuração administrativa | Backend recusa; leitura/aplicação continua funcionando. |
| Aceite salvo e cache removido | Fluxo obtém o hash exato antes de ficar pronto; falhas são visíveis. |
| Hash incorreto, pacote revogado/incompatível ou runtime falha | Não executa conteúdo substituto nem apresenta estado pronto falso. |
| Vários switches + SALVAR, concorrência e clique duplo | Uma aplicação do conjunto; conflito detectado; falha mantém anterior. |
| Atualizar MOD de eventos durante hospedagem | Eventos e pedidos passam a usar a mesma revisão e hash; nenhum runtime antigo continua ativo. |
| Evento e pedido alteram estado simultaneamente | Sem perda de atualização; commit antigo é serializado ou recusado. |
| Dois servidores no mesmo endereço e com chave TLS herdada | Identidades lógicas e aceites separados. |
| Migração interrompida, disco cheio, dados compartilhados ambíguos | Originais preservados, retomada possível e atribuição sem vazamento entre servidores. |
| Trocar versão do SEELE e retornar | Isolamento e compatibilidade de dados preservados; sem migração reversa implícita. |

Executar testes de ciclo de vida com tarefas atrasadas controladas, testes Rust de resolução/armazenamento/transação, conformance host/convidado e E2E do aplicativo empacotado em macOS e Windows. Preview web e disparar `unload` manualmente são auxiliares, não substitutos do fluxo real de saída.

Registrar em logs estruturados: instância, geração, conjunto, MOD/hash e transições de fase; evitar conteúdo privado. Diagnóstico deve distinguir pacote disponível, instalado no servidor, ativo no servidor, inicializando no cliente, pronto, falhou e encerrado.

## 10. Critério final de entrega

A infraestrutura estará alinhada ao pedido quando **nenhum MOD tiver autoridade ou runtime ativo fora da sessão correspondente**, dois servidores puderem usar versões e arquivos independentes, e a configuração continuar no servidor mesmo que todos os clientes saiam ou apaguem seu cache. A presença de bytes verificados no computador do participante é aceitável; ela não pode conceder ativação, configuração global ou efeito residual.

Entregar a correção imediata acompanhada dos testes de saída, depois a arquitetura com migração e os três MODs oficiais adaptados. Só considerar o trabalho completo após validar a matriz no aplicativo nativo e distinguir explicitamente os pacotes/versões realmente testados.
