module Icon exposing (..)

import Html
import Html.Attributes as AT


type Icon
    = Wifi
    | Exclamation
    | Circle
    | CircleFull
    | ChevronRight
    | ChevronLeft


view : Icon -> Html.Html m
view icon =
    case icon of
        ChevronLeft ->
            Html.div [ AT.class "icon fa-solid fa-chevron-left" ] []

        ChevronRight ->
            Html.div [ AT.class "icon fa-solid fa-chevron-right" ] []

        CircleFull ->
            Html.div [ AT.class "icon fa-solid fa-circle" ] []

        Circle ->
            Html.div [ AT.class "icon fa-regular fa-circle" ] []

        Wifi ->
            Html.div [ AT.class "icon fa-solid fa-wifi" ] []

        Exclamation ->
            Html.div [ AT.class "icon fa-solid fa-exclamation" ] []
