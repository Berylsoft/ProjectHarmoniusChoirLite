export function init(sid, selPass, selGroup, submit) {
  let pass = false;
  selPass.onchange = (s) => {
    pass = s[1];
    if (pass) selGroupContainer.style.display = "block";
    else selGroupContainer.style.display = "none";
  };

  let lead = false;
  let harmony = false;
  selGroup.onchange = (s) => {
    s[0] = true;
    lead = s[1];
    harmony = s[2];
    return s;
  };

  submit.onclick = () => {
    const body = {
      sid: sid,
      action: {},
    };

    if (pass) {
      const groups = ["Choir"];

      if (lead) groups.push("Lead");
      if (harmony) groups.push("Harmony");

      body.action = {
        Pass: groups,
      };
    } else {
      body.action = {
        Reject: null,
      };
    }

    fetch("/manager/review", {
      method: "POST",
      headers: {
        "content-type": "application/json",
      },
      body: JSON.stringify(body),
    }).then(async (res) => {
      if (res.status != 200) {
        const msg = await res.text();
        const reqId = res.headers["x-request-id"];
        console.log(msg, res);
        alert(`${res.status} ${res.statusText}, ${reqId}, ${msg}`);
      } else {
        globalThis.location.reload();
      }
    });
  };
}
