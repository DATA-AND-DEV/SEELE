// A escolha entre transmissões, exercitada no DOM.
//
// O que os guardas de texto não alcançam: se a fileira some com uma transmissão
// só, se a que está no palco é a marcada, e se a própria transmissão de quem
// compartilha fica de fora da escolha.
//
// `telaEmCurso` é escrito por `abrirImagemDaTela`, quando um fluxo abre de
// verdade. Aqui ele é posto à mão porque o que se mede é o desenho, e abrir um
// fluxo pediria um servidor.

const roster = [
  { id: 10, nickname: "rafa" },
  { id: 20, nickname: "marcela" },
  { id: 30, nickname: "eu" },
];

const semNenhuma = { roster, transmissoes: [] };
const soUma = { roster, transmissoes: [{ tela: 7, de: 10, e_minha: false }] };
const duas = {
  roster,
  transmissoes: [
    { tela: 7, de: 10, e_minha: false },
    { tela: 8, de: 20, e_minha: false },
  ],
};
const umaMinhaUmaAlheia = {
  roster,
  transmissoes: [
    { tela: 7, de: 10, e_minha: false },
    { tela: 9, de: 30, e_minha: true },
  ],
};

function estado() {
  const onde = $("palco-escolha");
  const rotulos = [...onde.querySelectorAll("button")].map(
    (b) => b.textContent + (b.getAttribute("aria-pressed") === "true" ? "*" : ""),
  );
  return (onde.hidden ? "escondida" : "visivel") + " [" + rotulos.join(" ") + "]";
}

desenharTransmissoes(semNenhuma);
relatar("ninguém transmitindo: " + estado());

desenharTransmissoes(soUma);
relatar("uma só: " + estado());

telaEmCurso = 7;
desenharTransmissoes(duas);
relatar("duas, recebendo a 7: " + estado());

telaEmCurso = 8;
desenharTransmissoes(duas);
relatar("duas, recebendo a 8: " + estado());

telaEmCurso = 7;
desenharTransmissoes(umaMinhaUmaAlheia);
relatar("a minha não entra na escolha: " + estado());

// Um id que o roster não conhece ainda: travessão, e nunca o número cru.
desenharTransmissoes({
  roster,
  transmissoes: [
    { tela: 7, de: 10, e_minha: false },
    { tela: 8, de: 99, e_minha: false },
  ],
});
relatar("quem o roster não tem: " + estado());
