export function init(sid, selPass, selGroup, submit, comment) {
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
    const encoded = new TextEncoder().encode(comment.value);
    if (encoded.length > 256) {
      const percent = ((encoded.length / 256) * 100 - 100).toFixed(0);
      alert(`备注过长, 超出${percent}%`);
      return;
    }

    const body = {
      sid: sid,
      action: {},
      comment: comment.value,
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

    function alertErrGroupMismatch() {
      let old;
      if (!lead) old = "领唱";
      else old = "和声";

      let now;
      if (lead) now = "领唱";
      else now = "和声";

      alert(`该用户上次通过时选择了"${old}"并且没有选择"${now}", \
不能同时缺少上次选择的组别和选择上次没有选择的组别, \
请考虑选择"${old}"或取消选择"${now}"`);
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
        const reqId = res.headers.get("x-request-id");
        console.log(msg, res);

        if (
          res.headers.get("content-type").startsWith("application/problem+json")
        ) {
          const problem = JSON.parse(msg);
          if (
            problem["type"].startsWith(
              "err://group_both_missing_old_and_have_new",
            )
          ) {
            alertErrGroupMismatch();
            return;
          }
        }

        alert(`${res.status} ${res.statusText}, ${reqId}, ${msg}`);
      } else {
        globalThis.location.reload();
      }
    });
  };
}
