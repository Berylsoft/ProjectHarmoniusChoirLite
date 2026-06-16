# bot

## token 生成

数据: 载荷用cbor序列化

签名: 数据用ed25519签名

token二进制: 签名 + 数据拼接

token: token二进制用base64编码

- base64字典: `ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_`
- base64需要无padding (尾部的=)

## 用户登录token

```jsonc
{
  "id": "string", // bot用来唯一识别一个用户的id, 只需要确保与用户唯一对应
  "is_manager": "boolean", // 有无审核权限
  "created_at": "string", // 当前时间用 RFC3339 格式化, 10分钟以内可用
}
```

## ws连接token

```jsonc
{
  "created_at": "string", // 当前时间用 RFC3339 格式化, 2分钟以内可用
}
```

## 用户登录

用户触发登录后 生成[用户登录token](#用户登录token), 拼接至url内发送

- url: `<prefix>/auth/login?token=<token>`

## 通知接收

生成 [ws连接token](#ws连接token), 拼接至url内发起连接

- url: `<prefix>/notify/bot?token=<token>`

ws 需要按
[RFC6455](https://datatracker.ietf.org/doc/html/rfc6455#section-5.5.2)
回复服务端的 ping

通知内容使用ws的二进制载荷, 用cbor反序列化

### 审核拒绝载荷

```jsonc
{
  "type": "Review",
  "data": {
    "id": "string", // 和bot签的登录token里的id一致
    "result": {
      "action": "Reject",
      "data": null,
    },
    "comment": "null | string" // 审核备注
  }
}
```

### 审核通过载荷

```jsonc
{
  "type": "Review",
  "data": {
    "id": "string", // 和bot签的登录token里的id一致
    "result": {
      "action": "Pass",
      "data": {
        "groups": ["Choir", "Lead", "Harmony"], // 通过的组
        "ignored": "boolean" // 有没有被忽略
      }
    },
    "comment": "null | string" // 审核备注
  }
}
```
