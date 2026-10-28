# Auditoria de domínios — 28/10/2026

Consulta somente leitura por SSH, DNS e HTTPS. Nenhuma configuração de produção foi alterada.

## Código e conteúdo

A 0.18 recusa novos convites nos domínios `seele.app.br` e subdomínios. Registros locais de servidores podem migrar o endereço do encontro offline, preservando a identidade, impressão e endereços conhecidos. O detalhe e os testes estão em `links-js.md` e nos testes Rust. Sites/documentação não têm links operacionais para a origem antiga; referências históricas são preservadas.

## Infraestrutura atual

| Domínio | Evidência pública |
| --- | --- |
| `seele.app.br` | Ainda resolve para Cloudflare e responde HTTPS 200. |
| `mods.seele.app.br` | Ainda resolve para Cloudflare e responde HTTPS 200. |
| `encontro.seele.app.br` | Ainda resolve para 216.128.168.216; handshake HTTPS recusado. |
| `docs.seele.app.br` | Não resolveu nesta consulta. |
| `seele.sh` | Resolve para Cloudflare. |
| `rendzvous.seele.sh` | Resolve para 216.128.168.216. |

O proxy ativo `seele-017-caddy.service` contém somente o virtual host `rendzvous.seele.sh`: sinalização para 127.0.0.1:8385 e resposta 404 nas demais rotas. O serviço ativo `seele-encontro.service` executa `/opt/seele/seele-encontro --sinal 127.0.0.1:8385`.

A unidade histórica `seele-017-encontro.service` ainda menciona `https://encontro.seele.app.br`, mas está **disabled e inactive**. Não é dependência ativa. Não houve serviço TURN ativo na listagem consultada.

## Fechamento no lançamento

A aposentadoria no produto está implementada; a aposentadoria pública **ainda não está concluída**. Na janela de publicação, remover os vínculos/rotas antigos no Cloudflare e os registros DNS antigos que ainda respondem, depois de conferir que não afetam os destinos `.sh`. Conferir novamente HTTPS e DNS após propagação. Não redirecionar convites antigos automaticamente: a política aprovada é pedir um convite novo e assumir nova identidade na origem web atual.

Manter as evidências e os guardas de produção que reconhecem domínios antigos. A mera presença de strings em histórico ou guardas não constitui serviço em funcionamento.
