export function init(token, btnLogin) {
  const re = () => /^#AL([a-zA-Z0-9-_]+)#AL$/;

  let clipboardFailed = false;

  token.onclick = () => {
    if (token.value === "") {
      navigator.clipboard
        .readText()
        .then((text) => {
          const match = re().exec(text);

          if (!match) {
            alert("无效的token, 请检查复制的是否正确");
            return;
          }

          token.value = match[1];
          btnLogin.click();
        })
        .catch((e) => {
          console.log(e);
          clipboardFailed = true;
        });
    } else if (!clipboardFailed) {
      token.value = "";
    }
  };

  token.oninput = () => {
    const match = re().exec(token.value);
    if (!match) return;
    token.value = match[1];
    btnLogin.click();
  };
}
