# PERFIS: imagens gravadas que não apareciam na v0.14.0

## Evidência e reprodução

Conferida a release v0.14.0, publicada em 21/09/2026 às 18:12 UTC a partir de `5fe17686dcd40c967d0864879cd8b1a743ea6bb4`. O log da instalação identifica o cliente 0.14.0 e duas seleções do PERFIS às 18:33 UTC: WebP com 134.236 bytes e JPEG com 292.997 bytes.

A inspeção somente de leitura do banco e dos índices de upload mostrou foto e banner associados ao perfil, sem upload pendente. Os índices continham 30 fragmentos/179.007 caracteres e 66 fragmentos/390.687 caracteres: os tamanhos completos das imagens em data URI. Nenhum dado da instalação foi alterado.

O envio chegou ao fim. Na exibição, `midia_em_bytes` passava o data URI inteiro para `de_base64`, que recusava o prefixo com `nao-e-base64`. O retrato escondia a recusa ao conservar a inicial; essa falha não era registrada no log. Um teste do caminho nativo reproduziu exatamente essa recusa antes do conserto.

Outras duas falhas foram reproduzidas: o renderer reutilizava o nó de mídia sem conferir mudança da fonte; e o rascunho do PERFIS preservava referências antigas de foto/banner, escondendo a imagem nova na prévia enquanto havia texto em edição.

## Correção

- SEELE aceita base64 puro e data URI base64 na leitura de mídia do servidor. O tipo vem dos bytes, independentemente do MIME alegado. Limites, geração e reconhecimento de formato continuam valendo. Recusas da conversão agora são registradas sem incluir a imagem.
- O renderer substitui o recurso quando a fonte muda ou desaparece. Libera contadores, cancela montagens atrasadas e preserva o nó quando a fonte permanece igual.
- PERFIS inclui a referência da imagem no pedido de mídia, distinguindo duas versões do mesmo campo. A prévia combina o rascunho de texto/aparência com as imagens já gravadas.
- O laboratório do PERFIS usa o laço de paginação real de `base.js`, em vez de apresentar somente a primeira página.

## Validação

Os testes novos falharam antes das correções: `nao-e-base64` na ponte nativa, nenhuma leitura ao acrescentar fonte ao retrato/mídia e prévia sem referência da foto enviada.

- `cargo test -p seele-app --bin seele-app a_supervisao_dos_mods_nativos`: 18 testes, incluindo PNG/JPEG/WebP, base64 puro/data URI, MIME alegado diferente dos bytes, entradas inválidas, 10 MiB e recusa acima do limite.
- `cargo test -p seele-app --test frontend`: 238 testes.
- `node apps/seele-app/bancada/regiao-do-mod.cjs`: 18 provas, incluindo substituição, remoção, resposta atrasada e preservação de mídia inalterada.
- PERFIS `npm test`: 41 testes.
- PERFIS `npm run test:ui`: upload pelo seletor do laboratório, servidor real do MOD, paginação real da janela e renderer real em Chromium. Foto, faixa e substituição ficam decodificadas (`naturalWidth`), inclusive na lista de pessoas, preservando texto em edição. O executor e o transporte nativos são simulados nessa bancada; a conversão nativa tem os testes Rust acima.

As mudanças são de código local em SEELE e PERFIS. Não houve publicação de release nem substituição do aplicativo instalado. A atualização do cliente SEELE é necessária; enviar somente o pacote do PERFIS não corrige a conversão nativa da v0.14.0. Imagens já gravadas não precisam de migração.
