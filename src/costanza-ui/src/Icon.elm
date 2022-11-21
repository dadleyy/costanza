module Icon exposing (..)

import Html
import Html.Attributes as AT


type Icon
    = Wifi
    | Exclamation


view : Icon -> Html.Html m
view icon =
    case icon of
        Wifi ->
            Html.div [ AT.class "icon fa-solid fa-wifi" ] []

        Exclamation ->
            Html.div [ AT.class "icon fa-solid fa-exclamation" ] []
