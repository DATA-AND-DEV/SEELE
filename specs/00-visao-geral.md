# 00 — Visão geral

## O problema

Ferramentas de voz para grupos pequenos convergiram para um formato só: aplicação Electron pesada, sempre online, com feed social, presença publicada e telemetria. Para quem trabalha em terminal, isso é um contexto estranho e caro em recursos.

## O produto

**SEELE** é um servidor de voz e texto auto-hospedado. O cliente principal, **Entry Plug** (`plug`), roda inteiramente no terminal. Há clientes gráficos para desktop e mobile que compartilham o mesmo núcleo.

Três coisas definem o projeto:

1. **Terminal como cidadão de primeira classe.** A TUI não é uma versão reduzida — é a referência. Os clientes gráficos é que imitam ela.
2. **Auto-hospedado e pequeno.** Um binário, um arquivo de configuração, um banco SQLite. Deve rodar confortavelmente em uma VPS de 1 vCPU / 512 MB atendendo ~50 usuários simultâneos.
3. **Identidade visual completa e coerente.** O tema Evangelion não é skin: é o vocabulário do produto inteiro, do nome dos canais às mensagens de erro. Ver `07-tema-evangelion.md`.

## Escopo — v1.0

- Voz em grupo com baixa latência (alvo: < 70 ms boca-a-ouvido em rede local, < 150 ms na internet regional).
- Canais de voz (**VoiceRooms**) e de texto (**Linhas**) persistentes.
- Mensagens de texto com histórico, edição e resposta.
- Push-to-talk e ativação por voz (VAD).
- Volume e mudo por usuário, no lado de quem escuta.
- Permissões por papel.
- Contas locais no servidor, sem dependência de terceiros.
- TUI completa e app desktop.

## Não-objetivos — v1.0

Registrados explicitamente para evitar deriva de escopo:

- Vídeo e compartilhamento de tela.
- Federação entre servidores.
- Bots, webhooks, marketplace de plugins.
- Threads, fóruns, reações, feed.
- App mobile em paridade completa (mobile em v1 é **somente consumo**: ouvir, falar, ler; ver `06-clientes-gui.md`).
- Descoberta pública de servidores.

## Critérios de sucesso

| Métrica | Alvo |
|---|---|
| Latência boca-a-ouvido, LAN | < 70 ms |
| Latência boca-a-ouvido, internet regional | < 150 ms |
| Uso de CPU do servidor, 20 usuários falando | < 15% de 1 vCPU |
| Uso de RSS do cliente TUI | < 60 MB |
| Tempo de boot até pronto para falar | < 1,5 s |
| Recuperação após queda de rede | Transparente em até 5 min |

> **Nota sobre a latência de LAN.** O número original era 60 ms, escrito antes de
> qualquer medição existir. `M1.1` mediu o piso real em ≈ 67 ms, e a maior parte
> dele é irredutível: 20 ms de acúmulo de quadro, 6,5 ms de lookahead do encoder
> e 19,6 ms do par de dispositivos. Detalhamento por estágio em
> `docs/adr/0009-orcamento-de-latencia.md`, medições em `docs/m1-medicoes.md`.
>
> A única alavanca restante seria quadro de 10 ms, que levaria o piso a ~57 ms
> ao custo de dobrar a taxa de pacotes. Foi considerada e recusada.

## Riscos conhecidos

- **Áudio em tempo real é a parte difícil.** Jitter buffer, eco e device handling multiplataforma consomem mais tempo que todo o resto somado. Prototipar cedo (M1), não deixar para o fim.
- **`cpal` tem comportamento divergente entre backends** (WASAPI, CoreAudio, ALSA/PipeWire). Testar nos três desde o começo.
- **Cancelamento de eco** não é resolvível de forma simples em Rust puro hoje. Ver `03-audio.md`, seção de decisões em aberto.
- **Escopo do tema.** É fácil gastar semanas em estética. O tema é definido uma vez em `07` e aplicado; não se redesenha a cada tela.
