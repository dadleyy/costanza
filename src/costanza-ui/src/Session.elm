module Session exposing (SessionData, SessionPayload, SessionUserData, decode)

import Json.Decode as JD


type alias SessionUserData =
    { name : String
    , picture : String
    , user_id : String
    }


type alias SessionData =
    { user : Maybe SessionUserData }


type alias SessionPayload =
    { ok : Bool
    , session : SessionData
    }


sessionUserDataDecoder : JD.Decoder SessionUserData
sessionUserDataDecoder =
    JD.map3 SessionUserData
        (JD.field "name" JD.string)
        (JD.field "picture" JD.string)
        (JD.field "user_id" JD.string)


sessionFieldDecoder : JD.Decoder SessionData
sessionFieldDecoder =
    JD.map SessionData (JD.nullable (JD.field "user" sessionUserDataDecoder))


decode : JD.Decoder SessionPayload
decode =
    JD.map2 SessionPayload
        (JD.field "ok" JD.bool)
        (JD.field "session" sessionFieldDecoder)
