module Routing exposing (..)

import Browser.Navigation as Nav
import Environment as Env
import HomePage
import Html
import Html.Attributes as AT
import Session
import Time
import Url


type Route
    = Home HomePage.HomePage
    | Loading


type Message
    = Tick Time.Posix
    | HomeMessage HomePage.Message
    | WebsocketMessage String


view : Route -> Html.Html Message
view route =
    case route of
        Loading ->
            Html.div [] []

        Home homePage ->
            Html.div [ AT.attribute "data-role" "route-container" ]
                [ HomePage.view homePage |> Html.map HomeMessage ]


authorized : Env.Environment -> Url.Url -> Nav.Key -> Session.SessionUserData -> ( Route, Cmd Message )
authorized env url key session =
    case String.startsWith "/home" url.path of
        True ->
            ( Home (HomePage.init key url), Cmd.none )

        False ->
            ( Loading, Nav.replaceUrl key "/home/terminal" )


update : Env.Environment -> Nav.Key -> Message -> Route -> ( Route, Cmd Message )
update env key message route =
    case ( message, route ) of
        ( _, Loading ) ->
            ( route, Cmd.none )

        ( WebsocketMessage content, Home homePage ) ->
            let
                ( newInner, routeCmd ) =
                    HomePage.update (HomePage.RawWebsocket content) ( homePage, env, key )
            in
            ( Home newInner, routeCmd |> Cmd.map HomeMessage )

        ( Tick posix, Home homePage ) ->
            let
                ( newInner, routeCmd ) =
                    HomePage.update (HomePage.Tick posix) ( homePage, env, key )
            in
            ( Home newInner, routeCmd |> Cmd.map HomeMessage )

        ( HomeMessage homeMessage, Home homePage ) ->
            let
                ( newInner, routeCmd ) =
                    HomePage.update homeMessage ( homePage, env, key )
            in
            ( Home newInner, routeCmd |> Cmd.map HomeMessage )
