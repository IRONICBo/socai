// Results stay visible by default; the user can inspect each step without autoplay.
document.querySelectorAll<HTMLElement>('[data-creator-workflow]').forEach(workflow => {
  const buttons = [...workflow.querySelectorAll<HTMLButtonElement>('[data-cs-step]')];
  const screens = [...workflow.querySelectorAll<HTMLElement>('[data-cs-screen]')];
  const address = workflow.querySelector<HTMLElement>('[data-cs-address]')!;
  buttons.forEach(button => button.addEventListener('click', () => {
    const selected = button.dataset.csStep;
    screens.forEach(screen => { screen.hidden = screen.dataset.csScreen !== selected; });
    buttons.forEach(other => other.setAttribute('aria-pressed', String(other === button)));
    address.textContent = button.dataset.address!;
  }));
});
