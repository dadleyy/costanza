module Environment exposing (..)


type alias Environment =
    { apiRoot : String
    , uiRoot : String
    , loginURL : String
    , logoutURL : String
    , version : String
    }
