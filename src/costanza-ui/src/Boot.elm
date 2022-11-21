port module Boot exposing (..)

import Json.Encode as JE


port sendMessage : String -> Cmd msg


port messageReceiver : (String -> msg) -> Sub msg


startWebsocket : Cmd a
startWebsocket =
    let
        message =
            JE.object [ ( "kind", JE.string "control" ) ]
    in
    sendMessage (JE.encode 0 message)
