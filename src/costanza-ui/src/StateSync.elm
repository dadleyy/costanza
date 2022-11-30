module StateSync exposing (ResponseContent, SentRequestKinds(..), StateHistoryEntry(..), StatePayload, StateSyncKinds(..), parseMessage)

import Json.Decode as JD


type alias StatePayload =
    { tick : Int
    , serialAvailable : Bool
    , history : List StateHistoryEntry
    }


type alias ResponseContent =
    { status : String
    , tick : Int
    }


type SentRequestKinds
    = RawSerial String
    | Unknown


type alias SentRequestEntry =
    { tick : Int
    , request : SentRequestKinds
    }


type StateSyncKinds
    = State StatePayload
    | Response ResponseContent


type StateHistoryEntry
    = SentRequest SentRequestEntry
    | ReceivedData String


sentRequestKindDecoder : String -> JD.Decoder SentRequestKinds
sentRequestKindDecoder payload =
    case payload of
        "raw_serial" ->
            JD.field "value" JD.string |> JD.andThen (\data -> JD.succeed (RawSerial data))

        _ ->
            JD.succeed Unknown


sentRequestDecoder : JD.Decoder SentRequestKinds
sentRequestDecoder =
    JD.field "kind" JD.string |> JD.andThen sentRequestKindDecoder


kindDecoder : String -> JD.Decoder StateHistoryEntry
kindDecoder kind =
    case kind of
        "received_data" ->
            JD.field "content" JD.string |> JD.map ReceivedData

        "sent_command" ->
            JD.map2 SentRequestEntry
                (JD.field "tick" JD.int)
                (JD.field "request" sentRequestDecoder)
                |> JD.map SentRequest

        _ ->
            JD.fail ("unrecognized history kind: " ++ kind)


historyDecoder : JD.Decoder StateHistoryEntry
historyDecoder =
    JD.field "history_kind" JD.string |> JD.andThen kindDecoder


parseMessage : String -> Result JD.Error StateSyncKinds
parseMessage payload =
    let
        parsedKind =
            JD.decodeString (JD.field "kind" JD.string) payload
    in
    case parsedKind of
        Ok "response" ->
            JD.decodeString
                (JD.map2 ResponseContent
                    (JD.field "status" JD.string)
                    (JD.field "tick" JD.int)
                )
                payload
                |> Result.map Response

        Ok "state" ->
            JD.decodeString
                (JD.map3 StatePayload
                    (JD.field "tick" JD.int)
                    (JD.field "serial_available" JD.bool)
                    (JD.field "history" (JD.list historyDecoder))
                )
                payload
                |> Result.map State

        Ok unrecognized ->
            JD.decodeString (JD.fail ("unrecognized top-level state sync kind: " ++ unrecognized)) ""

        Err error ->
            Err error
