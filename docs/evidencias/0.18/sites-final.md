# Sites — auditoria final da árvore 0.18

Data: 28/10/2026. Validação local; nenhuma publicação, assinatura, release, build Cargo ou operação Git nesta auditoria.

## Resultado

- Produto: `npm run build` passou usando dependências existentes.
- Docs: `SEELE_RAIZ=… .venv/bin/python -m pytest ferramentas/testes/test_gerar_docs.py -q`: 5 passaram, 1 ignorado (sem condição aberta para testar o rascunho).
- Catálogo/guia: `node --test site/testes/guia.test.js site/testes/tela.test.js`: 31 passaram; inclui avaliação/hash/commit por versão e revogações.
- Docs/catálogo: 40 cenas em 320, 390, 720, 768 e 1440 px; zero overflow horizontal, controles recortados ou erros JS. Exercitados menu/histórico, ordenação, Mesa 4.0.1 e todos os 28 capítulos do guia. 720 px cobre o reflow equivalente a 1440 px com zoom de 200%.
- Produto: 390/768/1440 px, zero overflow de documento/conteúdo ou recorte de cabeçalho/ações/imagem.
- Busca local `codigo` sem acento encontra o guia 0.18; abrir o resultado funciona. Os seis destinos do caminho rápido existem.
- Identificação ativa: **0.18 em desenvolvimento**, guia `/usar-a-0-18/`; guia anterior `/usar-a-0-17/` permanece histórico. Downloads continuam vinculados à release pública.

Pré-visualizações existentes reutilizadas: produto 5176, catálogo 5174 e docs 5175. Playwright do runtime existente; nenhuma instalação de dependências.

Artefatos locais: `/private/tmp/seele-sites-018/{docs-mods-controles.json,produto-larguras.json,busca-links.json}` e JPGs desktop/mobile na mesma pasta. Scripts usados: `/private/tmp/seele-docs-mods-final018.cjs`, `/private/tmp/seele-produto-final018.cjs`, `/private/tmp/seele-sites-018/busca-links.cjs`.

## Domínios e preservações

Nenhum `href`, `src` ou `action` ativo com `seele.app.br` ou `web.seele.sh` nos HTMLs de `site`, `publicado` (fora dos pacotes MOD) e `site-docs`; nenhuma URL aposentada ativa encontrada na UI/web do aplicativo. Não houve correção de URL necessária.

Ocorrências preservadas: migração de identidade em `SEELE-SITE/src/dados/privacidade.ts`; texto histórico do guia até 0.17.3; planos/entregas antigos do indexador; comentários CSS que citam a antiga origem. A busca indexa artigos históricos e pode, corretamente, encontrar essas menções. Isso não cria links ativos para eles.

- Produto/Downloads: `https://get.seele.sh/` e `/#baixar`.
- Docs: `https://docs.seele.sh/`, `/usar-a-0-18/` e `/mods/`.
- Catálogo: `https://mods.seele.sh/` e rotas de Estilo/Mesa/Perfis.
- Navegador e origem de convites: `https://seele.sh/` (`pessoal-web-entrada.js`).
- Serviço: `https://rendzvous.seele.sh/chamada/1` e `stun:rendzvous.seele.sh:8384` (`chamada-web.js`).

Conferência é de links/configuração e navegação local; não afirma implantação da 0.18 nos domínios públicos. Catálogo assinado, revogações, pacotes MOD e releases publicadas não foram alterados.

## Pacote web

Os testes `pacote-web-carregamento.cjs` e `pacote-web-cache.cjs` terminaram com sucesso em Chromium/WebKit usando os artefatos já presentes, antes do aviso do root sobre o rebuild pendente. **Esse target anterior não homologa o pacote final 0.18.** O root recompilará após o ajuste Rust legado e repetirá a conferência do pacote. Nenhum build Cargo foi disparado nesta frente.

## Scripts 0.17.3 reutilizáveis — somente leitura

Base: `/Users/dev-alexandre/SEELE-RELEASES/preparacao-0.17.3/`. Nenhum script foi executado nem alterado nesta auditoria; os scripts originais permanecem como evidência da 0.17.3.

| Arquivo | Papel e adaptação necessária para 0.18 |
| --- | --- |
| `conferir-fonte-mac.py` | Gate de bytes da fonte contra `EXPECTED_COMMIT`, inclui fontes ignoradas e registra exceções de evidência. Fixar novo destino/raiz e SHA autorizado; árvore de implementação ainda alterada reprovará o gate. |
| `empacotar-mac.sh` | Build Mac arm64 sem chave updater; usa o gate antes/depois, target existente e runner/cargo-guardado da preparação 0.17. Alterar versão/config/destinos; integrar o novo preparo WASM nativo antes de executar. |
| `gerar-dmg.py` | Confere versão do bundle e cria/verifica DMG. Alterar versão/destino; recusa sobrescrever arquivo existente. |
| `finalizar-mac.py` | Confere bundle id/arm64/codesign/sidecar/proveniência e gera arquivos determinísticos comparando cada payload SHA256. Alterar versão e caminhos; não é somente leitura. |
| `windows/build.ps1` | Recebe `ExpectedCommit`, exige checkout limpo/main, faz pull, carrega VS18/LLVM, build com 2 jobs e logs/exit. Versão 0.17.3, nomes e caminhos de evidência são fixos. Adaptar cópia nova; não executar original contra a preparação atual. |
| `windows/verificar.ps1` | Confere exit/logs/commit/checkout restaurado, versão dos dois executáveis e hashes dos 3 artefatos. Alterar versão, nomes e destinos de logs/resultado. |
| `windows/conferir-baixados.py` | Confere tamanho/SHA256 dos 3 artefatos e identidade dos executáveis dentro de portátil/CLI. Alterar versão e arquivo de resultado; grava relatórios. |
| `conferir-desktop.py` | Verifica assinaturas e hashes de Mac/Windows; também regenera/escreve manifesto, notas e SHA256SUMS. Trocar SHA fixo `eb233ed…`, versão/nomes/destinos; não usar como mero teste read-only. |

As etapas de assinatura/publicação não foram executadas. Os wrappers antigos devem ser adaptados em preparação nova e conferidos contra o fluxo WASM/CSP atual, preservando integralmente a entrega 0.17.3.
